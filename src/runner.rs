use std::time::{Duration, Instant};

use anyhow::Context;
use log::{debug, info};
use thousands::Separable;
use tokio::runtime::Builder;
use tokio::time;

use crate::config::AppConfig;
use crate::param::ParamResolver;
use crate::transport::Transport;
use crate::vuser::{Session, VirtualUser, VirtualUserCommand};

/// 실행
pub struct Runner {
    config: AppConfig,
    interface: String,
    eps: u64,
}

impl Runner {
    pub fn new(config: AppConfig, interface: String, eps: u64) -> Self {
        Self {
            config,
            interface,
            eps,
        }
    }

    /// Virtual User를 생성하고 테스트 실행
    pub fn run(self) -> Result<(), anyhow::Error> {
        anyhow::ensure!(self.eps > 0, "eps must be greater than 0");

        let runtime = Builder::new_multi_thread().enable_time().build()?;
        runtime.block_on(self.run_internal())
    }

    async fn run_internal(self) -> Result<(), anyhow::Error> {
        let params = ParamResolver::new(self.config.params.clone());
        let transport = Transport::open(
            &self.interface,
            self.config.src.clone(),
            self.config.dst.clone(),
        )?;
        let vusers = (0..self.config.test.vu)
            .map(|id| VirtualUser::spawn(id, transport.clone()))
            .collect::<Vec<_>>();

        anyhow::ensure!(!vusers.is_empty(), "test.vu must be greater than 0");

        let mut next_vuser = 0usize;

        info!("runner started: vusers={} eps={}", vusers.len(), self.eps,);

        for vuser in &vusers {
            debug!(
                "vuser={} assigned client={}:{} client_mac={}",
                vuser.id, vuser.client.ip, vuser.client.port, vuser.client.mac
            );
        }

        let mut last_stats = transport.stats();
        let mut last_stats_at = Instant::now();

        loop {
            let started_at = Instant::now();

            for _ in 0..self.eps {
                let vuser = &vusers[next_vuser];
                next_vuser = (next_vuser + 1) % vusers.len();

                let session = Session::new(&self.config.http, &params);
                vuser
                    .tx
                    .send(VirtualUserCommand::HttpRequest(session))
                    .await
                    .with_context(|| format!("failed to dispatch session to vuser {}", vuser.id))?;
            }

            let dispatch_elapsed = started_at.elapsed();
            let delay_msg;

            if dispatch_elapsed < Duration::from_secs(1) {
                time::sleep(Duration::from_secs(1) - dispatch_elapsed).await;
                delay_msg = "";
            } else {
                delay_msg = ", eps delayed";
            }

            let now = Instant::now();
            let current_stats = transport.stats();
            let stats_elapsed = now.duration_since(last_stats_at).as_secs();
            let sessions = current_stats.sessions.saturating_sub(last_stats.sessions);
            let actual_eps = if stats_elapsed > 0 {
                sessions / stats_elapsed
            } else {
                0
            };

            info!(
                "stats vu={} tx_packets={} tx_bytes={} actual_eps={:.2} {}",
                vusers.len().separate_with_commas(),
                current_stats.packets.separate_with_commas(),
                current_stats.bytes.separate_with_commas(),
                actual_eps,
                delay_msg,
            );

            last_stats = current_stats;
            last_stats_at = now;
        }
    }
}
