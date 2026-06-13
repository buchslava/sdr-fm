use futuresdr::prelude::*;

use super::silence;

pub enum DspCommand {
    TuneFrequency(u32),
}

pub(super) async fn apply_command(handle: &FlowgraphHandle, src: BlockId, cmd: DspCommand) {
    let (port, pmt) = match cmd {
        DspCommand::TuneFrequency(hz) => ("freq", Pmt::F64(hz as f64)),
    };

    let fut = handle.post(src, port, pmt);
    let _ = silence::silence_during_async(fut).await;
}
