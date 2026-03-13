use crate::keyboard::{KEYBOARD_CHANNEL, KeyboardEvent};
use crate::mouse::{MOUSE_CHANNEL, MouseEvent};
use core::ops::ControlFlow;
use dpedal_config::{ComputerInput, DPedalControl, Mapping, MappingMode};
use embassy_time::Instant;

use crate::input::State;

#[derive(Clone, Copy)]
pub struct MappingState {
    phase: MappingPhase,
    /// Index of the next output_sequence item to process.
    output_index: u8,
    /// Index of the first currently-held output. Items in held_from..output_index are "active".
    /// Updated when AfterMillisRelease releases a segment.
    held_from: u8,
    /// Set when advance_outputs parks at an AfterMillis* control.
    /// Cleared when the timer expires or the sequence is cancelled.
    waiting_since: Option<Instant>,
}

impl MappingState {
    pub fn new() -> Self {
        MappingState {
            phase: MappingPhase::Released,
            output_index: 0,
            held_from: 0,
            waiting_since: None,
        }
    }

    /// Process one tick for a single mapping.
    /// ControlFlow::Continue returns the modified MappingState
    /// ControlFlow::Break indicates that all mapping states are now invalid and need to be recreated.
    pub async fn process(
        mut self,
        mapping: &Mapping,
        outputs: &[ComputerInput],
        all_pressed: bool,
        state: &mut State,
    ) -> ControlFlow<(), MappingState> {
        // Shared sequence-processing transition: continue advancing OutputsActive
        // sequences (including those parked at an AfterMillis* timer).
        self.advance_outputs(outputs, state).await?;

        // Mode-specific state machine transitions
        // For macro modes with completed sequence: release and transition
        if mapping.mode.is_macro() {
            self.phase = match self.phase {
                MappingPhase::OutputsActive if (self.output_index as usize) >= outputs.len() => {
                    self.release_held(outputs).await;
                    match mapping.mode {
                        MappingMode::MacroOnRelease => MappingPhase::Released,
                        _ => MappingPhase::AwaitingRelease,
                    }
                }
                _ => self.phase,
            };
        }

        self.phase = match mapping.mode {
            MappingMode::OnPressUntilRelease => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => {
                    self.start_outputs(outputs, state).await?;
                    MappingPhase::OutputsActive
                }
                (MappingPhase::OutputsActive, false) => {
                    self.release_held(outputs).await;
                    MappingPhase::Released
                }
                (other, _) => other,
            },

            MappingMode::OnHoldMillisUntilRelease(threshold) => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => MappingPhase::HeldPending {
                    since: Instant::now(),
                },
                (MappingPhase::HeldPending { since }, true) => {
                    if since.elapsed().as_millis() >= threshold as u64 {
                        self.start_outputs(outputs, state).await?;
                        MappingPhase::OutputsActive
                    } else {
                        self.phase
                    }
                }
                (MappingPhase::HeldPending { .. }, false) => MappingPhase::Released,
                (MappingPhase::OutputsActive, false) => {
                    self.release_held(outputs).await;
                    MappingPhase::Released
                }
                (other, _) => other,
            },

            MappingMode::Toggle => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => {
                    self.start_outputs(outputs, state).await?;
                    if (self.output_index as usize) >= outputs.len() {
                        MappingPhase::ToggleOnAwaitingRelease
                    } else {
                        MappingPhase::OutputsActive
                    }
                }
                (MappingPhase::ToggleOnAwaitingRelease, false) => MappingPhase::OutputsActive,
                (MappingPhase::OutputsActive, true) => {
                    self.release_held(outputs).await;
                    MappingPhase::AwaitingRelease
                }
                (MappingPhase::AwaitingRelease, false) => MappingPhase::Released,
                (other, _) => other,
            },

            MappingMode::MacroOnPress => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => {
                    self.start_outputs(outputs, state).await?;
                    MappingPhase::OutputsActive
                }
                (MappingPhase::AwaitingRelease, false) => MappingPhase::Released,
                (other, _) => other,
            },

            MappingMode::MacroOnRelease => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => MappingPhase::MacroOnReleaseArmed,
                (MappingPhase::MacroOnReleaseArmed, false) => {
                    self.start_outputs(outputs, state).await?;
                    MappingPhase::OutputsActive
                }
                (other, _) => other,
            },

            MappingMode::MacroOnTapMillis(threshold) => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => MappingPhase::HeldPending {
                    since: Instant::now(),
                },
                (MappingPhase::HeldPending { since }, true) => {
                    if since.elapsed().as_millis() >= threshold as u64 {
                        MappingPhase::AwaitingRelease
                    } else {
                        self.phase
                    }
                }
                (MappingPhase::HeldPending { .. }, false) => {
                    self.start_outputs(outputs, state).await?;
                    MappingPhase::OutputsActive
                }
                (MappingPhase::AwaitingRelease, false) => MappingPhase::Released,
                (other, _) => other,
            },

            MappingMode::MacroOnDoubleTapMillis(threshold) => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => MappingPhase::HeldPending {
                    since: Instant::now(),
                },
                (MappingPhase::HeldPending { since }, true) => {
                    if since.elapsed().as_millis() >= threshold as u64 {
                        MappingPhase::AwaitingRelease
                    } else {
                        self.phase
                    }
                }
                (MappingPhase::HeldPending { since }, false) => {
                    MappingPhase::DoubleTapGap { since }
                }
                (MappingPhase::DoubleTapGap { since }, false) => {
                    if since.elapsed().as_millis() >= threshold as u64 {
                        MappingPhase::Released
                    } else {
                        self.phase
                    }
                }
                (MappingPhase::DoubleTapGap { since }, true) => {
                    MappingPhase::DoubleTapSecondPressed { since }
                }
                (MappingPhase::DoubleTapSecondPressed { since }, true) => {
                    if since.elapsed().as_millis() >= threshold as u64 {
                        MappingPhase::AwaitingRelease
                    } else {
                        self.phase
                    }
                }
                (MappingPhase::DoubleTapSecondPressed { .. }, false) => {
                    self.start_outputs(outputs, state).await?;
                    MappingPhase::OutputsActive
                }
                (MappingPhase::AwaitingRelease, false) => MappingPhase::Released,
                (other, _) => other,
            },

            MappingMode::MacroOnHoldMillis(threshold) => match (self.phase, all_pressed) {
                (MappingPhase::Released, true) => MappingPhase::HeldPending {
                    since: Instant::now(),
                },
                (MappingPhase::HeldPending { since }, true) => {
                    if since.elapsed().as_millis() >= threshold as u64 {
                        self.start_outputs(outputs, state).await?;
                        MappingPhase::OutputsActive
                    } else {
                        self.phase
                    }
                }
                (MappingPhase::HeldPending { .. }, false) => MappingPhase::Released,
                (MappingPhase::AwaitingRelease, false) => MappingPhase::Released,
                (other, _) => other,
            },
        };

        ControlFlow::Continue(self)
    }

    /// If in OutputsActive phase with items remaining, advance the output sequence one step.
    /// Returns Break if SetProfile fired (caller must `continue 'main_loop`).
    async fn advance_outputs(
        &mut self,
        outputs: &[ComputerInput],
        state: &mut State,
    ) -> ControlFlow<()> {
        if let MappingPhase::OutputsActive = self.phase
            && (self.output_index as usize) < outputs.len()
        {
            return self.run_sequence(outputs, state).await;
        }
        ControlFlow::Continue(())
    }

    /// Process the output_sequence starting from output_index, pressing keyboard/mouse items
    /// until hitting an AfterMillis* control (timer not yet expired), Restart, SetProfile,
    /// or the end of the sequence. AfterMillis* timer state is stored in waiting_since.
    async fn run_sequence(
        &mut self,
        outputs: &[ComputerInput],
        state: &mut State,
    ) -> ControlFlow<()> {
        while (self.output_index as usize) < outputs.len() {
            match &outputs[self.output_index as usize] {
                ComputerInput::Keyboard(key) => {
                    KEYBOARD_CHANNEL.send(KeyboardEvent::Pressed(*key)).await;
                    self.output_index += 1;
                }
                ComputerInput::Mouse(mouse) => {
                    MOUSE_CHANNEL.send(MouseEvent::Pressed(*mouse)).await;
                    self.output_index += 1;
                }
                ComputerInput::Control(DPedalControl::AfterMillisHold(millis)) => {
                    let millis = *millis;
                    let since = match self.waiting_since {
                        Some(s) => s,
                        None => {
                            self.waiting_since = Some(Instant::now());
                            return ControlFlow::Continue(());
                        }
                    };
                    if since.elapsed().as_millis() < millis as u64 {
                        return ControlFlow::Continue(());
                    }
                    // Timer expired; keep previously held items held (held_from unchanged).
                    self.waiting_since = None;
                    self.output_index += 1;
                }
                ComputerInput::Control(DPedalControl::AfterMillisRelease(millis)) => {
                    let millis = *millis;
                    let since = match self.waiting_since {
                        Some(s) => s,
                        None => {
                            self.waiting_since = Some(Instant::now());
                            return ControlFlow::Continue(());
                        }
                    };
                    if since.elapsed().as_millis() < millis as u64 {
                        return ControlFlow::Continue(());
                    }
                    // Timer expired; release outputs[held_from..output_index].
                    let start = self.held_from as usize;
                    let end = self.output_index as usize;
                    for output in &outputs[start..end] {
                        match output {
                            ComputerInput::Keyboard(key) => {
                                KEYBOARD_CHANNEL.send(KeyboardEvent::Released(*key)).await;
                            }
                            ComputerInput::Mouse(mouse) => {
                                MOUSE_CHANNEL.send(MouseEvent::Released(*mouse)).await;
                            }
                            ComputerInput::Control(_) => {}
                        }
                    }
                    self.held_from = self.output_index + 1;
                    self.waiting_since = None;
                    self.output_index += 1;
                }
                ComputerInput::Control(DPedalControl::Restart) => {
                    self.output_index = 0;
                    self.held_from = 0;
                    return ControlFlow::Continue(());
                }
                ComputerInput::Control(DPedalControl::SetProfile(profile)) => {
                    let profile = *profile;
                    self.output_index += 1;
                    state.current_profile = profile;
                    state.mapping_states.clear();
                    return ControlFlow::Break(());
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// Release keyboard/mouse items in outputs[held_from..output_index].
    /// Resets output_index, held_from, and waiting_since to their zero/None values.
    async fn release_held(&mut self, outputs: &[ComputerInput]) {
        let start = self.held_from as usize;
        let end = (self.output_index as usize).min(outputs.len());
        for output in &outputs[start..end] {
            match output {
                ComputerInput::Keyboard(key) => {
                    KEYBOARD_CHANNEL.send(KeyboardEvent::Released(*key)).await;
                }
                ComputerInput::Mouse(mouse) => {
                    MOUSE_CHANNEL.send(MouseEvent::Released(*mouse)).await;
                }
                ComputerInput::Control(_) => {}
            }
        }
        self.output_index = 0;
        self.held_from = 0;
        self.waiting_since = None;
    }

    /// Reset output_index and held_from to 0, then run the output sequence.
    async fn start_outputs(
        &mut self,
        outputs: &[ComputerInput],
        state: &mut State,
    ) -> ControlFlow<()> {
        self.output_index = 0;
        self.held_from = 0;
        self.run_sequence(outputs, state).await
    }
}

#[derive(Clone, Copy)]
enum MappingPhase {
    /// Idle. No inputs held, no pending activity.
    Released,
    /// Inputs held, timing started. Used by hold/tap/double-tap modes.
    HeldPending { since: Instant },
    /// Outputs are being pressed / held. If output_index < sequence length,
    /// advance_outputs will be called each tick to continue processing
    /// (including waiting for an AfterMillis* timer stored in MappingState.waiting_since).
    OutputsActive,
    /// Toggle activated, waiting for physical release before accepting next toggle press.
    ToggleOnAwaitingRelease,
    /// Waiting for full release before returning to Released.
    AwaitingRelease,
    /// MacroOnRelease: inputs pressed, will fire macro on release.
    MacroOnReleaseArmed,
    /// DoubleTap: first tap complete, awaiting second press.
    DoubleTapGap { since: Instant },
    /// DoubleTap: second press, awaiting second release to fire.
    DoubleTapSecondPressed { since: Instant },
}
