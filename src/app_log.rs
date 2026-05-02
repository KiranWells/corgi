use parking_lot::RwLock;
use tracing::Subscriber;
use tracing_subscriber::Layer;

static LOGS: RwLock<Vec<AppLog>> = RwLock::new(vec![]);

pub struct AppLog {
    pub level: tracing::Level,
    pub message: String,
    pub time: std::time::Instant,
}
pub struct AppSubscriber {}

impl<S: Subscriber> Layer<S> for AppSubscriber {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if *event.metadata().level() <= tracing::Level::WARN
            && event
                .metadata()
                .module_path()
                .is_some_and(|p| p.split_once(':').is_some_and(|x| x.0 == "corgi"))
            && let Some(mut logs) = LOGS.try_write()
        {
            logs.push(AppLog::from(event))
        }
    }
}

impl<'a> From<&tracing::Event<'a>> for AppLog {
    fn from(value: &tracing::Event<'a>) -> Self {
        struct MsgVisitor {
            msg: String,
        }
        impl tracing::field::Visit for MsgVisitor {
            fn record_debug(
                &mut self,
                field: &tracing::field::Field,
                value: &dyn core::fmt::Debug,
            ) {
                if field.name() == "message" {
                    self.msg = format!("{value:?}");
                }
            }
        }
        let mut mv = MsgVisitor { msg: String::new() };
        value.record(&mut mv);
        AppLog {
            level: *value.metadata().level(),
            message: mv.msg,
            time: std::time::Instant::now(),
        }
    }
}

/// Allows handling the global stored logs. DO NOT log new messages within this scope
pub fn logs_mut(callback: impl FnOnce(&mut Vec<AppLog>)) {
    callback(&mut LOGS.write());
}
