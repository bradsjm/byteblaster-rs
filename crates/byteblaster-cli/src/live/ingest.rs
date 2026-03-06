use crate::ReceiverKind;
use crate::live::config::{LiveConfigRequest, LiveReceiverConfig, build_live_receiver_config};
use byteblaster_core::ingest::{IngestConfig, IngestError, IngestEvent, IngestReceiver};
use futures::Stream;
use std::pin::Pin;

pub(crate) type IngestEventStream =
    Pin<Box<dyn Stream<Item = Result<IngestEvent, IngestError>> + Send + 'static>>;

pub(crate) struct LiveIngestRequest<'a> {
    pub(crate) live: &'a crate::LiveOptions,
    pub(crate) qbt_watchdog_timeout_secs: u64,
    pub(crate) username_context: &'static str,
    pub(crate) password_context: &'static str,
}

pub(crate) struct LiveIngest {
    receiver_kind: ReceiverKind,
    receiver: IngestReceiver,
}

impl LiveIngest {
    pub(crate) fn build(request: LiveIngestRequest<'_>) -> crate::error::CliResult<Self> {
        let live = request.live;
        let (receiver_kind, config) = match live.receiver {
            ReceiverKind::Qbt => {
                let LiveReceiverConfig::Qbt(config) =
                    build_live_receiver_config(LiveConfigRequest {
                        receiver: ReceiverKind::Qbt,
                        username: live.username.clone(),
                        password: live.password.clone(),
                        raw_servers: live.servers.clone(),
                        server_list_path: live.server_list_path.clone(),
                        idle_timeout_secs: live.idle_timeout_secs,
                        qbt_watchdog_timeout_secs: request.qbt_watchdog_timeout_secs,
                        username_context: request.username_context,
                        password_context: request.password_context,
                    })?
                else {
                    unreachable!("qbt request must build qbt config");
                };
                (ReceiverKind::Qbt, IngestConfig::Qbt(config))
            }
            ReceiverKind::Wxwire => {
                let LiveReceiverConfig::WxWire(config) =
                    build_live_receiver_config(LiveConfigRequest {
                        receiver: ReceiverKind::Wxwire,
                        username: live.username.clone(),
                        password: live.password.clone(),
                        raw_servers: live.servers.clone(),
                        server_list_path: live.server_list_path.clone(),
                        idle_timeout_secs: live.idle_timeout_secs,
                        qbt_watchdog_timeout_secs: 0,
                        username_context: request.username_context,
                        password_context: request.password_context,
                    })?
                else {
                    unreachable!("wxwire request must build wxwire config");
                };
                (ReceiverKind::Wxwire, IngestConfig::WxWire(config))
            }
        };

        Ok(Self {
            receiver_kind,
            receiver: IngestReceiver::build(config)?,
        })
    }

    pub(crate) fn receiver_kind(&self) -> ReceiverKind {
        self.receiver_kind
    }

    pub(crate) fn start(&mut self) -> crate::error::CliResult<()> {
        self.receiver.start().map_err(Into::into)
    }

    pub(crate) fn events(&mut self) -> Result<IngestEventStream, IngestError> {
        self.receiver.events()
    }

    pub(crate) async fn stop(&mut self) -> crate::error::CliResult<()> {
        self.receiver.stop().await.map_err(Into::into)
    }
}
