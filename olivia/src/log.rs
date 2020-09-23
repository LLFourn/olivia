use crate::oracle::{EventResult, OutcomeResult};

pub trait OracleLog {
    fn log_event_result(&self, res: Result<(), EventResult>);
    fn log_outcome_result(&self, res: Result<(), OutcomeResult>);
}

impl OracleLog for slog::Logger {
    fn log_event_result(&self, res: Result<(), EventResult>) {
        use EventResult::*;
        match res {
            Ok(_) => info!(self, "created"),
            Err(e) => match e {
                Changed => info!(self, "changed"),
                AlreadyExists => debug!(self, "ignored - already exists"),
                AlreadyCompleted => debug!(self, "ignored - already completed"),
                DbReadErr(e) => crit!(self,"database read";"error" => format!("{}",e)),
                DbWriteErr(e) => crit!(self,"database write"; "error" => format!("{}", e)),
            },
        }
    }

    fn log_outcome_result(&self, res: Result<(), OutcomeResult>) {
        use OutcomeResult::*;
        match res {
            Ok(_) => info!(self, "completed"),
            Err(e) => match e {
                AlreadyCompleted => debug!(self, "already completed"),
                OutcomeChanged { existing, new } => {
                    crit!(self, "outcome changed"; "existing" => existing, "new" => new)
                }
                EventNotExist => error!(self, "event doesn't exist"),
                DbReadErr(e) => crit!(self, "database read"; "error" => format!("{}", e)),
                DbWriteErr(e) => crit!(self, "database write"; "error" => format!("{}", e)),
            },
        }
    }
}
