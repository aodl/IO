use crate::state::{self, Lifecycle};

pub fn set_paused(paused: bool) -> Result<(), String> {
    let mut state = state::read();
    if !paused && state.active_operation.is_some() {
        return Err("cannot unpause with active NNS operation".into());
    }
    state.lifecycle = if paused {
        Lifecycle::Paused
    } else {
        Lifecycle::Ready
    };
    state::write(state);
    Ok(())
}
