//! Service Mesh Module
//!
//! 统一的服务注册发现中心，接入链上 DID Resolver。
//! 提供 Agent 服务的注册、发现、路由和健康检查功能。
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    AgentServiceMesh                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
//! │  │   Service    │  │   Service    │  │    Health    │      │
//! │  │   Registry   │  │   Resolver   │  │   Monitor    │      │
//! │  └──────────────┘  └──────────────┘  └──────────────┘      │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    DID Resolver (Chain)                     │
//! └─────────────────────────────────────────────────────────────┘
//! ```

// Note: AgentCard and DiscoveryService used in future extensions
use std::sync::Arc;

use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::did::{DIDDocument, DIDResolver};
use crate::error::{AgentError, Result};

pub mod health;
pub mod registry;
pub mod resolver;
pub mod routing;

pub use health::{HealthChecker, HealthStatus};
pub use registry::{ServiceEntry, ServiceRegistry, ServiceState};
pub use resolver::ServiceResolver;
pub use routing::{LoadBalancer, RoutingStrategy, ServiceRouter};

/// Service Mesh 配置
#[derive(Debug, Clone)]
pub struct ServiceMeshConfig {
    /// 服务健康检查间隔（秒）
    pub health_check_interval_secs: u64,
    /// 服务超时时间（秒）
    pub service_timeout_secs: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 负载均衡策略
    pub load_balance_strategy: LoadBalanceStrategy,
    /// 是否启用链上验证
    pub enable_chain_verification: bool,
    /// 缓存 TTL（秒）
    pub cache_ttl_secs: u64,
}

impl Default for ServiceMeshConfig {
    fn default() -> Self {
        Self {
            health_check_interval_secs: 180,
            service_timeout_secs: 180,
            max_retries: 3,
            load_balance_strategy: LoadBalanceStrategy::RoundRobin,
            enable_chain_verification: true,
            cache_ttl_secs: 300,
        }
    }
}

/// 负载均衡策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalanceStrategy {
    /// 轮询
    RoundRobin,
    /// 随机
    Random,
    /// 最少连接
    LeastConnections,
    /// 加权轮询
    WeightedRoundRobin,
    /// 一致性哈希
    ConsistentHash,
}

/// Agent Service Mesh - 统一的服务注册发现中心
///
/// 集成链上 DID Resolver，提供服务注册、发现、路由和健康检查功能。
pub struct AgentServiceMesh {
    /// 服务注册表
    registry: Arc<dyn ServiceRegistry>,
    /// DID Resolver - 接入链上身份验证
    did_resolver: Arc<DIDResolver>,
    /// 服务解析器
    service_resolver: ServiceResolver,
    /// 健康检查器
    health_checker: Arc<HealthChecker>,
    /// 服务路由器
    router: ServiceRouter,
    /// 配置
    config: ServiceMeshConfig,
    /// 本地服务实例 ID
    local_instance_id: String,
}

impl AgentServiceMesh {
    /// 创建新的 Service Mesh 实例
    pub fn new(registry: Arc<dyn ServiceRegistry>, did_resolver: Arc<DIDResolver>) -> Self {
        let config = ServiceMeshConfig::default();
        let local_instance_id = Uuid::new_v4().to_string();

        let service_resolver = ServiceResolver::new(registry.clone(), did_resolver.clone());

        let health_checker = Arc::new(HealthChecker::new(
            registry.clone(),
            config.health_check_interval_secs,
        ));

        let router = ServiceRouter::new(service_resolver.clone(), config.load_balance_strategy);

        Self {
            registry,
            did_resolver,
            service_resolver,
            health_checker,
            router,
            config,
            local_instance_id,
        }
    }

    /// 使用配置创建 Service Mesh
    pub fn with_config(
        registry: Arc<dyn ServiceRegistry>,
        did_resolver: Arc<DIDResolver>,
        config: ServiceMeshConfig,
    ) -> Self {
        let local_instance_id = Uuid::new_v4().to_string();

        let service_resolver = ServiceResolver::new(registry.clone(), did_resolver.clone());

        let health_checker = Arc::new(HealthChecker::new(
            registry.clone(),
            config.health_check_interval_secs,
        ));

        let router = ServiceRouter::new(service_resolver.clone(), config.load_balance_strategy);

        Self {
            registry,
            did_resolver,
            service_resolver,
            health_checker,
            router,
            config,
            local_instance_id,
        }
    }

    /// 获取本地实例 ID
    pub fn local_instance_id(&self) -> &str {
        &self.local_instance_id
    }

    /// 注册服务
    ///
    /// 如果启用链上验证，会首先验证 DID
    pub async fn register_service(&self, service_entry: ServiceEntry) -> Result<()> {
        info!(
            "Registering service: {} (DID: {:?})",
            service_entry.id, service_entry.did
        );

        // 如果启用链上验证且提供了 DID
        if self.config.enable_chain_verification {
            if let Some(ref did) = service_entry.did {
                match self.verify_did_on_chain(did).await {
                    Ok(doc) => {
                        info!(
                            "DID {} verified on chain: verified={}",
                            did, doc.on_chain_verified
                        );
                    }
                    Err(e) => {
                        warn!("DID verification failed for {}: {}", did, e);
                        return Err(AgentError::InvalidConfig(format!(
                            "DID verification failed: {}",
                            e
                        )));
                    }
                }
            }
        }

        self.registry
            .register(service_entry)
            .await
            .map_err(|e| AgentError::ServiceMesh(format!("Failed to register service: {}", e)))
    }

    /// 注销服务
    pub async fn deregister_service(&self, service_id: &str) -> Result<()> {
        info!("Deregistering service: {}", service_id);

        self.registry
            .deregister(service_id)
            .await
            .map_err(|e| AgentError::ServiceMesh(format!("Failed to deregister service: {}", e)))
    }

    /// 发现服务
    ///
    /// 根据服务名称发现可用的服务实例
    pub async fn discover_service(&self, service_name: &str) -> Result<Vec<ServiceEntry>> {
        debug!("Discovering service: {}", service_name);

        let services = self
            .service_resolver
            .resolve_by_name(service_name)
            .await
            .map_err(|e| AgentError::ServiceMesh(format!("Service discovery failed: {}", e)))?;

        // 过滤掉不健康的服务
        let healthy_services: Vec<_> = services
            .into_iter()
            .filter(|s| s.state == ServiceState::Healthy)
            .collect();

        Ok(healthy_services)
    }

    /// 通过 DID 发现服务
    ///
    /// 解析 DID 并找到对应的服务
    pub async fn discover_by_did(&self, did: &str) -> Result<Option<ServiceEntry>> {
        debug!("Discovering service by DID: {}", did);

        // 解析 DID
        let _doc = self
            .did_resolver
            .resolve(did)
            .await
            .map_err(|e| AgentError::DIDResolution(e.to_string()))?;

        // 查找与该 DID 关联的服务
        self.service_resolver
            .resolve_by_did(did)
            .await
            .map_err(|e| AgentError::ServiceMesh(format!("DID service discovery failed: {}", e)))
    }

    /// 路由请求到服务
    ///
    /// 使用负载均衡策略选择一个健康的服务实例
    pub async fn route_request(
        &self,
        service_name: &str,
        routing_key: Option<&str>,
    ) -> Result<ServiceEntry> {
        self.router
            .route(service_name, routing_key)
            .await
            .map_err(|e| AgentError::ServiceMesh(format!("Routing failed: {}", e)))
    }

    /// 更新服务健康状态
    pub async fn update_health(&self, service_id: &str, healthy: bool) -> Result<()> {
        let state = if healthy {
            ServiceState::Healthy
        } else {
            ServiceState::Unhealthy
        };

        self.registry
            .update_state(service_id, state)
            .await
            .map_err(|e| AgentError::ServiceMesh(format!("Failed to update health: {}", e)))
    }

    /// 启动健康检查
    pub async fn start_health_checks(&self) {
        info!(
            "Starting health checks with interval {}s",
            self.config.health_check_interval_secs
        );

        self.health_checker.start().await;
    }

    /// 停止健康检查
    pub async fn stop_health_checks(&self) {
        info!("Stopping health checks");

        self.health_checker.stop().await;
    }

    /// 获取服务统计信息
    pub async fn get_stats(&self) -> ServiceMeshStats {
        let services = match self.registry.list_all().await {
            Ok(s) => s,
            Err(_) => vec![],
        };

        let total = services.len();
        let healthy = services
            .iter()
            .filter(|s| s.state == ServiceState::Healthy)
            .count();
        let unhealthy = services
            .iter()
            .filter(|s| s.state == ServiceState::Unhealthy)
            .count();
        let registering = services
            .iter()
            .filter(|s| s.state == ServiceState::Registering)
            .count();

        ServiceMeshStats {
            total_services: total,
            healthy_services: healthy,
            unhealthy_services: unhealthy,
            registering_services: registering,
            local_instance_id: self.local_instance_id.clone(),
        }
    }

    /// 验证 DID 在链上
    async fn verify_did_on_chain(&self, did: &str) -> Result<DIDDocument> {
        self.did_resolver
            .resolve(did)
            .await
            .map_err(|e| AgentError::DIDResolution(e.to_string()))
    }

    /// 获取服务注册表引用
    pub fn registry(&self) -> &Arc<dyn ServiceRegistry> {
        &self.registry
    }

    /// 获取 DID Resolver 引用
    pub fn did_resolver(&self) -> &Arc<DIDResolver> {
        &self.did_resolver
    }

    /// 获取服务路由器
    pub fn router(&self) -> &ServiceRouter {
        &self.router
    }
}

/// Service Mesh 统计信息
#[derive(Debug, Clone)]
pub struct ServiceMeshStats {
    /// 总服务数
    pub total_services: usize,
    /// 健康服务数
    pub healthy_services: usize,
    /// 不健康服务数
    pub unhealthy_services: usize,
    /// 注册中服务数
    pub registering_services: usize,
    /// 本地实例 ID
    pub local_instance_id: String,
}

impl ServiceMeshStats {
    /// 计算健康率
    pub fn health_rate(&self) -> f64 {
        if self.total_services == 0 {
            1.0
        } else {
            self.healthy_services as f64 / self.total_services as f64
        }
    }
}

/// Service Mesh 构建器
pub struct ServiceMeshBuilder {
    registry: Option<Arc<dyn ServiceRegistry>>,
    did_resolver: Option<Arc<DIDResolver>>,
    config: ServiceMeshConfig,
}

impl ServiceMeshBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            registry: None,
            did_resolver: None,
            config: ServiceMeshConfig::default(),
        }
    }

    /// 设置服务注册表
    pub fn with_registry(mut self, registry: Arc<dyn ServiceRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// 设置 DID Resolver
    pub fn with_did_resolver(mut self, resolver: Arc<DIDResolver>) -> Self {
        self.did_resolver = Some(resolver);
        self
    }

    /// 设置配置
    pub fn with_config(mut self, config: ServiceMeshConfig) -> Self {
        self.config = config;
        self
    }

    /// 设置健康检查间隔
    pub fn health_check_interval(mut self, secs: u64) -> Self {
        self.config.health_check_interval_secs = secs;
        self
    }

    /// 设置负载均衡策略
    pub fn load_balance_strategy(mut self, strategy: LoadBalanceStrategy) -> Self {
        self.config.load_balance_strategy = strategy;
        self
    }

    /// 启用/禁用链上验证
    pub fn enable_chain_verification(mut self, enable: bool) -> Self {
        self.config.enable_chain_verification = enable;
        self
    }

    /// 构建 Service Mesh
    pub fn build(self) -> Result<AgentServiceMesh> {
        let registry = self
            .registry
            .ok_or_else(|| AgentError::InvalidConfig("Service registry is required".to_string()))?;

        let did_resolver = self
            .did_resolver
            .ok_or_else(|| AgentError::InvalidConfig("DID resolver is required".to_string()))?;

        Ok(AgentServiceMesh::with_config(
            registry,
            did_resolver,
            self.config,
        ))
    }
}

impl Default for ServiceMeshBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_mesh_config_default() {
        let config = ServiceMeshConfig::default();
        assert_eq!(config.health_check_interval_secs, 30);
        assert_eq!(config.service_timeout_secs, 180);
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_chain_verification);
    }

    #[tokio::test]
    async fn test_service_mesh_stats() {
        let stats = ServiceMeshStats {
            total_services: 10,
            healthy_services: 8,
            unhealthy_services: 1,
            registering_services: 1,
            local_instance_id: "test".to_string(),
        };

        assert_eq!(stats.health_rate(), 0.8);
    }

    #[tokio::test]
    async fn test_service_mesh_stats_empty() {
        let stats = ServiceMeshStats {
            total_services: 0,
            healthy_services: 0,
            unhealthy_services: 0,
            registering_services: 0,
            local_instance_id: "test".to_string(),
        };

        assert_eq!(stats.health_rate(), 1.0);
    }
}
