use arc_swap::ArcSwap;
use pingora_load_balancing::{LoadBalancer, selection::RoundRobin};
use std::sync::Arc;

pub struct ServiceDiscovery {
    pub worker_lb: ArcSwap<LoadBalancer<RoundRobin>>,
    pub auth_lb: ArcSwap<LoadBalancer<RoundRobin>>,
}

impl ServiceDiscovery {
    pub fn new(
        initial_worker_lb: LoadBalancer<RoundRobin>,
        initial_auth_lb: LoadBalancer<RoundRobin>,
    ) -> Self {
        Self {
            worker_lb: ArcSwap::from_pointee(initial_worker_lb),
            auth_lb: ArcSwap::from_pointee(initial_auth_lb),
        }
    }
}

pub fn create_empty_lb() -> anyhow::Result<LoadBalancer<RoundRobin>> {
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

async fn watch_service(
    consul_addr: String,
    service_name: &'static str,
    target: Arc<dyn Fn(LoadBalancer<RoundRobin>) + Send + Sync>,
) {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/health/service/{}?passing=true", consul_addr, service_name);
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
                            tracing::info!("Healthy {}s updated in Consul: {:?}", service_name, ips);
                            last_ips = ips.clone();
                            
                            match LoadBalancer::<RoundRobin>::try_from_iter(ips) {
                                Ok(mut lb) => {
                                    lb.set_health_check(pingora_load_balancing::health_check::TcpHealthCheck::new());
                                    lb.health_check_frequency = Some(std::time::Duration::from_secs(5));
                                    
                                    target(lb);
                                }
                                Err(e) => tracing::error!("Failed to initialize load balancer for {}: {}", service_name, e),
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse Consul response for {}: {}", service_name, e);
                    }
                }
            }
            Ok(res) => tracing::error!("Consul returned error for {}: {}", service_name, res.status()),
            Err(e) => tracing::error!("Failed to fetch {} from Consul: {}", service_name, e),
        }
    }
}

pub async fn spawn_discovery_watcher(
    consul_addr: String,
    discovery: Arc<ServiceDiscovery>,
) -> anyhow::Result<()> {
    let disc_worker = discovery.clone();
    tokio::spawn(watch_service(
        consul_addr.clone(),
        "validation_worker",
        Arc::new(move |lb| {
            disc_worker.worker_lb.store(Arc::new(lb));
        }),
    ));

    let disc_auth = discovery.clone();
    tokio::spawn(watch_service(
        consul_addr,
        "auth_worker",
        Arc::new(move |lb| {
            disc_auth.auth_lb.store(Arc::new(lb));
        }),
    ));

    Ok(())
}
