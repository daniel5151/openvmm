// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! PS/2 mouse.

use futures::Stream;
use input_core::InputSource;
use input_core::MouseData;
use inspect::Inspect;
use spec::Ps2MouseCommand;
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// PS/2 mouse definitions.
mod spec {
    use bitfield_struct::bitfield;
    use inspect::Inspect;
    use open_enum::open_enum;

    open_enum! {
        #[derive(Inspect)]
        #[inspect(debug)]
        pub enum Ps2MouseCommand: u8 {
            SET_SCALING_1_1        = 0xE6,
            SET_SCALING_2_1        = 0xE7,
            SET_RESOLUTION         = 0xE8, // has data byte
            STATUS                 = 0xE9,
            SET_STREAM_MODE        = 0xEA,
            READ_DATA              = 0xEB,
            RESET_WRAP_MODE        = 0xEC,
            SET_WRAP_MODE          = 0xEE,
            SET_REMOTE_MODE        = 0xF0,
            GET_ID                 = 0xF2,
            SET_SAMPLE_RATE        = 0xF3, // has data byte
            ENABLE_DATA_REPORTING  = 0xF4,
            DISABLE_DATA_REPORTING = 0xF5,
            SET_DEFAULTS           = 0xF6,
            RESEND                 = 0xFE,
            RESET                  = 0xFF,
        }
    }

    #[derive(Inspect)]
    #[bitfield(u8)]
    pub struct MousePacketHeader {
        pub button_left: bool,
        pub button_right: bool,
        pub button_middle: bool,
        pub always_one: bool,
        pub x_sign: bool,
        pub y_sign: bool,
        pub x_overflow: bool,
        pub y_overflow: bool,
    }

    pub const ACKNOWLEDGE_COMMAND: u8 = 0xFA;
}

/// Not yet implemented.
#[derive(Inspect)]
pub struct Ps2Mouse {
    #[inspect(skip)]
    mouse_input: Box<dyn InputSource<MouseData>>,
    state: MouseState,
}

#[derive(Inspect)]
struct MouseState {
    previous_command: Option<Ps2MouseCommand>,
    #[inspect(bytes)]
    output_buffer: VecDeque<u8>,
    active: bool,
    last_output_byte_read: u8,

    prev_x: u16,
    prev_y: u16,
}

impl MouseState {
    fn new() -> Self {
        Self {
            previous_command: None,
            output_buffer: VecDeque::new(),
            active: false,
            last_output_byte_read: 0,
            prev_x: 0,
            prev_y: 0,
        }
    }
}

const MOUSE_BUFFER_SIZE: usize = 65;

impl Ps2Mouse {
    pub fn new(mouse_input: Box<dyn InputSource<MouseData>>) -> Self {
        Self {
            mouse_input,
            state: MouseState::new(),
        }
    }

    pub fn reset(&mut self) {
        self.state = MouseState::new();
    }

    pub fn poll(&mut self, cx: &mut Context<'_>) {
        while self.state.output_buffer.len() < MOUSE_BUFFER_SIZE - 4 {
            if let Poll::Ready(Some(input)) = Pin::new(&mut self.mouse_input).poll_next(cx) {
                if !self.state.active {
                    continue;
                }

                let mut dx = -(self.state.prev_x as i32 - input.x as i32);
                let mut dy = self.state.prev_y as i32 - input.y as i32;

                self.state.prev_x = input.x;
                self.state.prev_y = input.y;

                dx /= 16; // heuristic
                dy /= 16; // heuristic

                let header = spec::MousePacketHeader::new()
                    .with_always_one(true)
                    .with_x_sign(dx.is_negative())
                    .with_y_sign(dy.is_negative())
                    .with_button_left(input.button_mask.left())
                    .with_button_middle(input.button_mask.middle())
                    .with_button_right(input.button_mask.right());

                tracing::trace!(?header, dx, dy, "mouse event");

                self.state.output_buffer.push_back(header.into_bits());
                self.state.output_buffer.push_back(dx as u8); // FIXME: bad clamp
                self.state.output_buffer.push_back(dy as u8); // FIXME: bad clamp
            } else {
                break;
            }
        }
    }

    pub fn output(&mut self) -> Option<u8> {
        let value = self.state.output_buffer.pop_front()?;
        self.state.last_output_byte_read = value;
        Some(value)
    }

    pub fn input(&mut self, input: u8) {
        let (command, data) = if let Some(command) = self.state.previous_command.take() {
            (command, Some(input))
        } else {
            (Ps2MouseCommand(input), None)
        };
        if self.command(command, data).is_none() {
            self.state.previous_command = Some(command);
        }
    }

    fn push(&mut self, value: u8) {
        if self.state.output_buffer.len() <= MOUSE_BUFFER_SIZE {
            self.state.output_buffer.push_back(value);
        } else {
            // Indicate buffer overflow.
            *self.state.output_buffer.back_mut().unwrap() = 0;
        }
    }

    fn command(&mut self, command: Ps2MouseCommand, data: Option<u8>) -> Option<()> {
        tracing::trace!(?command, "mouse command");

        match command {
            Ps2MouseCommand::SET_SAMPLE_RATE => {
                self.push(spec::ACKNOWLEDGE_COMMAND);
                let sample_rate = data?;
                tracing::debug!(sample_rate, "set sample rate");
            }
            Ps2MouseCommand::SET_RESOLUTION => {
                self.push(spec::ACKNOWLEDGE_COMMAND);
                let resolution = data?;
                tracing::debug!(resolution, "set resolution");
            }
            Ps2MouseCommand::ENABLE_DATA_REPORTING => {
                self.push(spec::ACKNOWLEDGE_COMMAND);
                self.state.active = true;
            }
            Ps2MouseCommand::DISABLE_DATA_REPORTING => {
                self.push(spec::ACKNOWLEDGE_COMMAND);
                self.state.active = false;
            }
            Ps2MouseCommand::GET_ID => {
                self.push(spec::ACKNOWLEDGE_COMMAND);
                self.push(0); // IDENTITY - standard PS/2 mouse
            }
            Ps2MouseCommand::RESEND => {
                self.push(self.state.last_output_byte_read);
            }
            Ps2MouseCommand::RESET => {
                self.push(spec::ACKNOWLEDGE_COMMAND);
                self.push(0xAA); // COMPLETE
                self.push(0); // IDENTITY - standard PS/2 mouse
            }
            _ => {
                tracing::debug!(?command, "unimplemented mouse command");
                self.push(spec::ACKNOWLEDGE_COMMAND); // ACKNOWLEDGE
            }
        }

        Some(())
    }
}

mod save_restore {
    use super::*;
    use vmcore::save_restore::RestoreError;
    use vmcore::save_restore::SaveError;
    use vmcore::save_restore::SaveRestore;

    mod state {
        use mesh::payload::Protobuf;
        use vmcore::save_restore::SavedStateRoot;

        #[derive(Protobuf, SavedStateRoot)]
        #[mesh(package = "chipset.i8042.mouse")]
        pub struct SavedState {
            #[mesh(1)]
            pub output_buffer: Vec<u8>,
        }
    }

    impl SaveRestore for Ps2Mouse {
        type SavedState = state::SavedState;

        fn save(&mut self) -> Result<Self::SavedState, SaveError> {
            let Self {
                mouse_input: _,
                state:
                    MouseState {
                        output_buffer,
                        // FIXME: save these
                        previous_command: _,
                        active: _,
                        last_output_byte_read: _,
                        prev_x: _,
                        prev_y: _,
                    },
            } = self;

            let saved_state = state::SavedState {
                output_buffer: output_buffer.iter().copied().collect(),
            };

            Ok(saved_state)
        }

        fn restore(&mut self, state: Self::SavedState) -> Result<(), RestoreError> {
            let state::SavedState { output_buffer } = state;

            self.state.output_buffer = output_buffer.into();

            Ok(())
        }
    }
}
