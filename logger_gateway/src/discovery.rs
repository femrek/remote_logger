use arc_swap::ArcSwap;
use pingora_load_balancing::{LoadBalancer, selection::RoundRobin};
use std::sync::Arc;

pub struct ServiceDiscovery {
    pub load_balancer: ArcSwap<LoadBalancer<RoundRobin>>,
}

impl ServiceDiscovery {
    pub fn new(initial_lb: LoadBalancer<RoundRobin>) -> Self {
        Self {
            load_balancer: ArcSwap::from_pointee(initial_lb),
        }
    }
}

pub fn load_initial_workers(_consul_addr: &str) -> anyhow::Result<LoadBalancer<RoundRobin>> {
    // Return an empty load balancer initially.
    // The watcher will populate it almost immediately via Consul.
    let mut lb = LoadBalancer::<RoundRobin>::try_from_iter(Vec::<String>::new()).map_err(|e| anyhow::anyhow!("LB init error: {}", e))?;
    lb.set_health_check(pingora_load_balancing::health_check::TcpHealthCheck::new());
    lb.health_check_frequency = Some(std::time::Duration::from_secs(5));
    
    Ok(lb)
}

#[derive(serde::Deserialize, Debug)]
struct ConsulHealthCheck {
    #[serde(rename = "Service")]
    service: ConsulService,
}

#[derive(serde::Deserialize, Debug)]
struct ConsulService {
    #[serde(rename = "Address")]
    address: String,
    #[serde(rename = "Port")]
    port: u16,
}

pub async fn spawn_discovery_watcher(
    consul_addr: String,
    discovery: Arc<ServiceDiscovery>,
) -> anyhow::Result<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/v1/health/service/validation_worker?passing=true", consul_addr);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        
        let mut last_ips: Vec<String> = Vec::new();
        
        loop {
            interval.tick().await;
            match client.get(&url).send().await {
                Ok(res) if res.status().is_success() => {
                    match res.json::<Vec<ConsulHealthCheck>>().await {
                        Ok(checks) => {
                            let mut ips: Vec<String> = checks.into_iter()
                                .map(|c| format!("{}:{}", c.service.address, c.service.port))
                                .collect();
                            ips.sort();
                                
                            if ips != last_ips {
                                tracing::info!("Healthy workers updated in Consul: {:?}", ips);
                                last_ips = ips.clone();
                                
                                match LoadBalancer::<RoundRobin>::try_from_iter(ips) {
                                    Ok(mut lb) => {
                                        lb.set_health_check(pingora_load_balancing::health_check::TcpHealthCheck::new());
                                        lb.health_check_frequency = Some(std::time::Duration::from_secs(5));
                                        
                                        discovery.load_balancer.store(Arc::new(lb));
                                    }
                                    Err(e) => tracing::error!("Failed to initialize load balancer from Consul IPs: {}", e),
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse Consul response: {}", e);
                        }
                    }
                }
                Ok(res) => tracing::error!("Consul returned error: {}", res.status()),
                Err(e) => tracing::error!("Failed to fetch from Consul: {}", e),
            }
        }
    });

    Ok(())
}
