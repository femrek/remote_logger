use arc_swap::ArcSwap;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use pingora_load_balancing::{LoadBalancer, selection::RoundRobin};
use std::sync::Arc;
use tokio::sync::watch;

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

pub fn load_initial_workers(path: &str) -> anyhow::Result<LoadBalancer<RoundRobin>> {
    let content = std::fs::read_to_string(path)?;
    let trimmed = content.trim();
    let ips: Vec<String> = if trimmed.starts_with('[') {
        serde_json::from_str(&content)?
    } else {
        content.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    };
    
    let mut lb = LoadBalancer::<RoundRobin>::try_from_iter(ips).map_err(|e| anyhow::anyhow!("LB init error: {}", e))?;
    lb.set_health_check(pingora_load_balancing::health_check::TcpHealthCheck::new());
    lb.health_check_frequency = Some(std::time::Duration::from_secs(5));
    
    Ok(lb)
}

pub async fn spawn_discovery_watcher(
    path: String,
    discovery: Arc<ServiceDiscovery>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = watch::channel(());

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    let _ = tx.send(());
                }
            }
        },
        Config::default(),
    )?;

    watcher.watch(std::path::Path::new(&path), RecursiveMode::NonRecursive)?;

    tokio::spawn(async move {
        // Hold onto the watcher so it doesn't get dropped
        let _watcher = watcher;
        
        while rx.changed().await.is_ok() {
            tracing::info!("workers file changed, reloading load balancer");
            
            // Read and parse file
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let trimmed = content.trim();
                    let ips: Vec<String> = if trimmed.starts_with('[') {
                        match serde_json::from_str::<Vec<String>>(&content) {
                            Ok(parsed) => parsed,
                            Err(e) => {
                                tracing::error!("Failed to parse JSON workers: {}", e);
                                continue;
                            }
                        }
                    } else {
                        content.lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty() && !l.starts_with('#'))
                            .collect()
                    };
                    
                    match LoadBalancer::<RoundRobin>::try_from_iter(ips) {
                        Ok(mut lb) => {
                            lb.set_health_check(pingora_load_balancing::health_check::TcpHealthCheck::new());
                            lb.health_check_frequency = Some(std::time::Duration::from_secs(5));
                            
                            // Swap the load balancer
                            discovery.load_balancer.store(Arc::new(lb));
                            tracing::info!("Load balancer reloaded successfully with new IPs");
                        }
                        Err(e) => tracing::error!("Failed to initialize load balancer from new IPs: {}", e),
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to read workers: {}", e);
                }
            }
        }
    });

    Ok(())
}
