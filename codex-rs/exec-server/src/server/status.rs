use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde::Serialize;

use crate::ExecServerRuntimePaths;

const SERVICE_NAME: &str = "codex-exec-server";
const UNKNOWN_METHOD_LABEL: &str = "__unknown__";

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct SessionStatusSnapshot {
    pub(crate) active: u64,
    pub(crate) detached: u64,
}

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct ProcessStatusSnapshot {
    pub(crate) starting: u64,
    pub(crate) running: u64,
    pub(crate) exited_retained: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct RequestMetricKey {
    method: String,
    result: &'static str,
}

#[derive(Debug)]
pub(crate) struct ExecServerStatusState {
    started_at: Instant,
    runtime_paths: ExecServerRuntimePaths,
    active_connections: AtomicU64,
    total_connections: AtomicU64,
    total_sessions_created: AtomicU64,
    total_processes_started: AtomicU64,
    total_requests: AtomicU64,
    total_request_successes: AtomicU64,
    total_request_failures: AtomicU64,
    requests_by_method: StdMutex<BTreeMap<RequestMetricKey, u64>>,
}

impl ExecServerStatusState {
    pub(crate) fn new(runtime_paths: ExecServerRuntimePaths) -> Arc<Self> {
        Arc::new(Self {
            started_at: Instant::now(),
            runtime_paths,
            active_connections: AtomicU64::new(0),
            total_connections: AtomicU64::new(0),
            total_sessions_created: AtomicU64::new(0),
            total_processes_started: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_request_successes: AtomicU64::new(0),
            total_request_failures: AtomicU64::new(0),
            requests_by_method: StdMutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn connection_opened(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        self.total_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn connection_closed(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn session_created(&self) {
        self.total_sessions_created.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn process_started(&self) {
        self.total_processes_started.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn request_succeeded(&self, method: &str) {
        self.record_request(method, "ok");
        self.total_request_successes.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn request_failed(&self, method: &str) {
        self.record_request(method, "error");
        self.total_request_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) async fn readiness(&self) -> Result<(), String> {
        helper_path_ready(self.runtime_paths.codex_self_exe.as_ref()).await?;
        if let Some(path) = &self.runtime_paths.codex_linux_sandbox_exe {
            helper_path_ready(path.as_ref()).await?;
        }
        Ok(())
    }

    pub(crate) async fn snapshot(
        &self,
        sessions: SessionStatusSnapshot,
        processes: ProcessStatusSnapshot,
    ) -> StatusResponse {
        let status = if self.readiness().await.is_ok() {
            ServiceStatus::Ready
        } else {
            ServiceStatus::NotReady
        };
        StatusResponse {
            service: SERVICE_NAME.to_string(),
            status,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            connections: ConnectionStatus {
                active: self.active_connections.load(Ordering::Relaxed),
                total: self.total_connections.load(Ordering::Relaxed),
            },
            sessions: SessionStatus {
                active: sessions.active,
                detached: sessions.detached,
                total_created: self.total_sessions_created.load(Ordering::Relaxed),
            },
            processes: ProcessStatus {
                starting: processes.starting,
                running: processes.running,
                exited_retained: processes.exited_retained,
                total_started: self.total_processes_started.load(Ordering::Relaxed),
            },
            requests: RequestStatus {
                total: self.total_requests.load(Ordering::Relaxed),
                succeeded: self.total_request_successes.load(Ordering::Relaxed),
                failed: self.total_request_failures.load(Ordering::Relaxed),
            },
            capabilities: CapabilitiesStatus {
                process: true,
                filesystem: true,
                http: true,
                metrics: true,
            },
        }
    }

    pub(crate) fn render_prometheus_metrics(&self, snapshot: &StatusResponse) -> String {
        let request_metrics = self
            .requests_by_method
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let mut output = String::new();
        output.push_str(
            "# HELP codex_exec_server_uptime_seconds Seconds since the exec-server started.\n",
        );
        output.push_str("# TYPE codex_exec_server_uptime_seconds gauge\n");
        output.push_str(&format!(
            "codex_exec_server_uptime_seconds {}\n",
            snapshot.uptime_seconds
        ));
        output.push_str(
            "# HELP codex_exec_server_connections Current and cumulative websocket connections.\n",
        );
        output.push_str("# TYPE codex_exec_server_connections gauge\n");
        output.push_str(&format!(
            "codex_exec_server_connections{{state=\"active\"}} {}\n",
            snapshot.connections.active
        ));
        output.push_str("# TYPE codex_exec_server_connections_total counter\n");
        output.push_str(&format!(
            "codex_exec_server_connections_total {}\n",
            snapshot.connections.total
        ));
        output.push_str(
            "# HELP codex_exec_server_sessions Current and cumulative exec-server sessions.\n",
        );
        output.push_str("# TYPE codex_exec_server_sessions gauge\n");
        output.push_str(&format!(
            "codex_exec_server_sessions{{state=\"active\"}} {}\n",
            snapshot.sessions.active
        ));
        output.push_str(&format!(
            "codex_exec_server_sessions{{state=\"detached\"}} {}\n",
            snapshot.sessions.detached
        ));
        output.push_str("# TYPE codex_exec_server_sessions_created_total counter\n");
        output.push_str(&format!(
            "codex_exec_server_sessions_created_total {}\n",
            snapshot.sessions.total_created
        ));
        output.push_str("# HELP codex_exec_server_processes Current managed process counts.\n");
        output.push_str("# TYPE codex_exec_server_processes gauge\n");
        output.push_str(&format!(
            "codex_exec_server_processes{{state=\"starting\"}} {}\n",
            snapshot.processes.starting
        ));
        output.push_str(&format!(
            "codex_exec_server_processes{{state=\"running\"}} {}\n",
            snapshot.processes.running
        ));
        output.push_str(&format!(
            "codex_exec_server_processes{{state=\"exited_retained\"}} {}\n",
            snapshot.processes.exited_retained
        ));
        output.push_str("# TYPE codex_exec_server_processes_started_total counter\n");
        output.push_str(&format!(
            "codex_exec_server_processes_started_total {}\n",
            snapshot.processes.total_started
        ));
        output.push_str("# HELP codex_exec_server_requests_total JSON-RPC requests handled by method and result.\n");
        output.push_str("# TYPE codex_exec_server_requests_total counter\n");
        for (key, value) in request_metrics {
            output.push_str(&format!(
                "codex_exec_server_requests_total{{method=\"{}\",result=\"{}\"}} {}\n",
                key.method, key.result, value
            ));
        }
        output
    }

    fn record_request(&self, method: &str, result: &'static str) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let method = match method {
            crate::protocol::INITIALIZE_METHOD
            | crate::protocol::EXEC_METHOD
            | crate::protocol::EXEC_READ_METHOD
            | crate::protocol::EXEC_WRITE_METHOD
            | crate::protocol::EXEC_TERMINATE_METHOD
            | crate::protocol::FS_READ_FILE_METHOD
            | crate::protocol::FS_WRITE_FILE_METHOD
            | crate::protocol::FS_CREATE_DIRECTORY_METHOD
            | crate::protocol::FS_GET_METADATA_METHOD
            | crate::protocol::FS_READ_DIRECTORY_METHOD
            | crate::protocol::FS_REMOVE_METHOD
            | crate::protocol::FS_COPY_METHOD
            | crate::protocol::HTTP_REQUEST_METHOD => method,
            _ => UNKNOWN_METHOD_LABEL,
        };
        let mut requests_by_method = self
            .requests_by_method
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *requests_by_method
            .entry(RequestMetricKey {
                method: method.to_string(),
                result,
            })
            .or_insert(0) += 1;
    }
}

async fn helper_path_ready(path: &std::path::Path) -> Result<(), String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|err| format!("helper path is unavailable: {err}"))?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err("helper path is not a file".to_string())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusResponse {
    pub(crate) service: String,
    pub(crate) status: ServiceStatus,
    pub(crate) version: String,
    pub(crate) uptime_seconds: u64,
    pub(crate) connections: ConnectionStatus,
    pub(crate) sessions: SessionStatus,
    pub(crate) processes: ProcessStatus,
    pub(crate) requests: RequestStatus,
    pub(crate) capabilities: CapabilitiesStatus,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ServiceStatus {
    Ready,
    NotReady,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionStatus {
    pub(crate) active: u64,
    pub(crate) total: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionStatus {
    pub(crate) active: u64,
    pub(crate) detached: u64,
    pub(crate) total_created: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessStatus {
    pub(crate) starting: u64,
    pub(crate) running: u64,
    pub(crate) exited_retained: u64,
    pub(crate) total_started: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestStatus {
    pub(crate) total: u64,
    pub(crate) succeeded: u64,
    pub(crate) failed: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilitiesStatus {
    pub(crate) process: bool,
    pub(crate) filesystem: bool,
    pub(crate) http: bool,
    pub(crate) metrics: bool,
}

#[cfg(test)]
mod tests {
    use super::ExecServerStatusState;
    use crate::ExecServerRuntimePaths;

    #[tokio::test]
    async fn readiness_rejects_missing_required_helper_path() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let missing = tempdir.path().join("missing-codex");
        let runtime_paths =
            ExecServerRuntimePaths::new(missing, /*codex_linux_sandbox_exe*/ None)
                .expect("runtime paths");
        let status = ExecServerStatusState::new(runtime_paths);

        let error = status
            .readiness()
            .await
            .expect_err("missing helper should make exec-server not ready");
        assert!(error.contains("helper path is unavailable"));
    }
}
