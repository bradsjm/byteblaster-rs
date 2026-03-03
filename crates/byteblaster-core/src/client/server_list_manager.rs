use crate::error::{CoreError, CoreResult};
use crate::protocol::model::ServerList;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ServerListManager {
    path: Option<PathBuf>,
    current: ServerList,
    index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedServerList {
    servers: Vec<(String, u16)>,
    sat_servers: Vec<(String, u16)>,
    version: String,
}

impl ServerListManager {
    pub fn new(path: Option<PathBuf>, default_servers: Vec<(String, u16)>) -> Self {
        Self {
            path,
            current: ServerList {
                servers: default_servers,
                sat_servers: Vec::new(),
            },
            index: 0,
        }
    }

    pub fn load(&mut self) -> CoreResult<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        if !path.exists() {
            return Ok(());
        }

        let bytes = fs::read(path)?;
        let persisted: PersistedServerList =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Lifecycle(e.to_string()))?;

        if !persisted.servers.is_empty() || !persisted.sat_servers.is_empty() {
            self.current = ServerList {
                servers: persisted.servers,
                sat_servers: persisted.sat_servers,
            };
            self.index = 0;
        }

        Ok(())
    }

    pub fn save(&self) -> CoreResult<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        save_atomic(path, &self.current)
    }

    pub fn apply_server_list(&mut self, list: ServerList) -> CoreResult<()> {
        if list.servers.is_empty() && list.sat_servers.is_empty() {
            return Err(CoreError::Lifecycle(
                "server list update contained no valid endpoints".to_string(),
            ));
        }

        self.current = list;
        self.index = 0;
        self.save()
    }

    pub fn next_endpoint(&mut self) -> Option<(String, u16)> {
        let combined_len = self.current.servers.len() + self.current.sat_servers.len();
        if combined_len == 0 {
            return None;
        }

        let endpoint = if self.index < self.current.servers.len() {
            self.current.servers[self.index].clone()
        } else {
            let sat_idx = self.index - self.current.servers.len();
            self.current.sat_servers[sat_idx].clone()
        };

        self.index = (self.index + 1) % combined_len;
        Some(endpoint)
    }

    pub fn reset_rotation(&mut self) {
        self.index = 0;
    }

    pub fn current(&self) -> &ServerList {
        &self.current
    }
}

fn save_atomic(path: &Path, server_list: &ServerList) -> CoreResult<()> {
    let persisted = PersistedServerList {
        servers: server_list.servers.clone(),
        sat_servers: server_list.sat_servers.clone(),
        version: "1.0".to_string(),
    };

    let data = serde_json::to_vec_pretty(&persisted)
        .map_err(|e| CoreError::Lifecycle(format!("failed to serialize server list: {e}")))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ServerListManager;
    use crate::protocol::model::ServerList;

    #[test]
    fn rotation_across_server_and_sat_lists() {
        let mut mgr = ServerListManager::new(None, vec![("a".to_string(), 1)]);
        mgr.apply_server_list(ServerList {
            servers: vec![("a".to_string(), 1), ("b".to_string(), 2)],
            sat_servers: vec![("sat".to_string(), 3)],
        })
        .expect("server list should apply");

        assert_eq!(mgr.next_endpoint(), Some(("a".to_string(), 1)));
        assert_eq!(mgr.next_endpoint(), Some(("b".to_string(), 2)));
        assert_eq!(mgr.next_endpoint(), Some(("sat".to_string(), 3)));
        assert_eq!(mgr.next_endpoint(), Some(("a".to_string(), 1)));
    }
}
