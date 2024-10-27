// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(dead_code)]

use self::packed_nums::*;
use zerocopy::AsBytes;
use zerocopy::FromBytes;
use zerocopy::FromZeroes;

#[allow(non_camel_case_types)]
mod packed_nums {
    pub type u16_be = zerocopy::U16<zerocopy::BigEndian>;
    pub type u32_be = zerocopy::U32<zerocopy::BigEndian>;
}

// As defined in https://github.com/rfbproto/rfbproto/blob/master/rfbproto.rst#handshaking-messages

#[repr(transparent)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct ProtocolVersion(pub [u8; 12]);

pub const PROTOCOL_VERSION_33: [u8; 12] = *b"RFB 003.003\n";
pub const PROTOCOL_VERSION_37: [u8; 12] = *b"RFB 003.007\n";
pub const PROTOCOL_VERSION_38: [u8; 12] = *b"RFB 003.008\n";

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct Security33 {
    pub padding: [u8; 3],
    pub security_type: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct Security37 {
    pub type_count: u8,
    // types: [u8; N]
}

pub const SECURITY_TYPE_INVALID: u8 = 0;
pub const SECURITY_TYPE_NONE: u8 = 1;
pub const SECURITY_TYPE_VNC_AUTHENTICATION: u8 = 2;
pub const SECURITY_TYPE_TIGHT: u8 = 16;
pub const SECURITY_TYPE_VENCRYPT: u8 = 19;

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct SecurityResult {
    pub status: u32_be,
}

pub const SECURITY_RESULT_STATUS_OK: u32 = 0;
pub const SECURITY_RESULT_STATUS_FAILED: u32 = 1;
pub const SECURITY_RESULT_STATUS_FAILED_TOO_MANY_ATTEMPTS: u32 = 2;

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct ClientInit {
    pub shared_flag: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct ServerInit {
    pub framebuffer_width: u16_be,
    pub framebuffer_height: u16_be,
    pub server_pixel_format: PixelFormat,
    pub name_length: u32_be,
    // name_string: [u8; N],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct PixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian_flag: u8,
    pub true_color_flag: u8,
    pub red_max: u16_be,
    pub green_max: u16_be,
    pub blue_max: u16_be,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
    pub padding: [u8; 3],
}

// Client to server messages

pub const CS_MESSAGE_SET_PIXEL_FORMAT: u8 = 0;
pub const CS_MESSAGE_SET_ENCODINGS: u8 = 2;
pub const CS_MESSAGE_FRAMEBUFFER_UPDATE_REQUEST: u8 = 3;
pub const CS_MESSAGE_KEY_EVENT: u8 = 4;
pub const CS_MESSAGE_POINTER_EVENT: u8 = 5;
pub const CS_MESSAGE_CLIENT_CUT_TEXT: u8 = 6;
pub const CS_MESSAGE_QEMU: u8 = 255;

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct SetPixelFormat {
    pub message_type: u8,
    pub padding: [u8; 3],
    pub pixel_format: PixelFormat,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct SetEncodings {
    pub message_type: u8,
    pub padding: u8,
    pub encoding_count: u16_be,
    // encoding_type: [i32_be; N],
}

open_enum::open_enum! {
    pub enum EncodingType: u32 {
        // Regular encodings
        RAW       = 0,
        COPY_RECT = 1,
        RRE       = 2,
        CO_RRE    = 4,
        HEXTILE   = 5,
        ZLIB      = 6,
        TIGHT     = 7,
        ZLIBHEX   = 8,
        ULTRA     = 9,
        ULTRA2    = 10,
        TRLE      = 15,
        ZRLE      = 16,
        ZYWRLE    = 17,  // Hitachi ZYWRLE
        H264      = 20,
        JPEG      = 21,
        JRLE      = 22,
        OPEN_H264 = 50,
        TIGHT_PNG = -260i32 as u32,

        // Pseudo-encodings
        JPEG_QUALITY_LEVEL_MIN        = -32i32 as u32,  // -23 to -32 range
        JPEG_QUALITY_LEVEL_MAX        = -23i32 as u32,
        DESKTOP_SIZE                  = -223i32 as u32,
        LAST_RECT                     = -224i32 as u32,
        CURSOR                        = -239i32 as u32,
        X_CURSOR                      = -240i32 as u32,
        COMPRESSION_LEVEL_MIN         = -256i32 as u32,  // -247 to -256 range
        COMPRESSION_LEVEL_MAX         = -247i32 as u32,
        QEMU_POINTER_MOTION_CHANGE    = -257i32 as u32,
        QEMU_EXTENDED_KEY_EVENT       = -258i32 as u32,
        QEMU_AUDIO                    = -259i32 as u32,
        QEMU_LED_STATE                = -261i32 as u32,
        GII                           = -305i32 as u32,
        DESKTOP_NAME                  = -307i32 as u32,
        EXTENDED_DESKTOP_SIZE         = -308i32 as u32,
        XVP                           = -309i32 as u32,
        FENCE                         = -312i32 as u32,
        CONTINUOUS_UPDATES            = -313i32 as u32,
        CURSOR_WITH_ALPHA             = -314i32 as u32,
        TIGHT_WITHOUT_ZLIB            = -317i32 as u32,
        JPEG_FINE_GRAINED_QUALITY_MIN = -512i32 as u32,  // -412 to -512 range
        JPEG_FINE_GRAINED_QUALITY_MAX = -412i32 as u32,
        JPEG_SUBSAMPLING_MIN          = -768i32 as u32,  // -763 to -768 range
        JPEG_SUBSAMPLING_MAX          = -763i32 as u32,

        // VMware specific encodings
        VMWARE_CURSOR                = 0x574d5664,
        VMWARE_CURSOR_STATE          = 0x574d5665,
        VMWARE_CURSOR_POSITION       = 0x574d5666,
        VMWARE_KEY_REPEAT            = 0x574d5667,
        VMWARE_LED_STATE             = 0x574d5668,
        VMWARE_DISPLAY_MODE_CHANGE   = 0x574d5669,
        VMWARE_VIRTUAL_MACHINE_STATE = 0x574d566a,

        // Extended clipboard
        EXTENDED_CLIPBOARD = 0xc0a1e5ce,

        // Additional registered encodings
        APPLE_RANGE_1_START = 1000,
        APPLE_RANGE_1_END   = 1002,
        APPLE_1011          = 1011,
        APPLE_RANGE_2_START = 1100,
        APPLE_RANGE_2_END   = 1105,
        REALVNC_RANGE_START = 1024,
        REALVNC_RANGE_END   = 1099,

        // Additional pseudo-encodings
        KEYBOARD_LED_STATE   = 0xfffe0000,
        SUPPORTED_MESSAGES   = 0xfffe0001,
        SUPPORTED_ENCODINGS  = 0xfffe0002,
        SERVER_IDENTITY      = 0xfffe0003,
        CACHE                = 0xffff0000,
        CACHE_ENABLE         = 0xffff0001,
        XOR_ZLIB             = 0xffff0002,
        XOR_MONO_RECT_ZLIB   = 0xffff0003,
        XOR_MULTI_COLOR_ZLIB = 0xffff0004,
        SOLID_COLOR          = 0xffff0005,
        XOR_ENABLE           = 0xffff0006,
        CACHE_ZIP            = 0xffff0007,
        SOL_MONO_ZIP         = 0xffff0008,
        ULTRA_ZIP            = 0xffff0009,
        SERVER_STATE         = 0xffff8000,
        ENABLE_KEEP_ALIVE    = 0xffff8001,
        FT_PROTOCOL_VERSION  = 0xffff8002,
        SESSION              = 0xffff8003,
    }
}

impl From<EncodingType> for u32_be {
    fn from(value: EncodingType) -> Self {
        value.0.into()
    }
}

impl From<u32_be> for EncodingType {
    fn from(value: u32_be) -> Self {
        EncodingType(value.into())
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct FramebufferUpdateRequest {
    pub message_type: u8,
    pub incremental: u8,
    pub x: u16_be,
    pub y: u16_be,
    pub width: u16_be,
    pub height: u16_be,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct KeyEvent {
    pub message_type: u8,
    pub down_flag: u8,
    pub padding: [u8; 2],
    pub key: u32_be,
}

#[bitfield_struct::bitfield(u8)]
#[derive(AsBytes, FromBytes, FromZeroes)]
pub struct PointerEventButtonMask {
    pub left: bool,
    pub middle: bool,
    pub right: bool,
    pub scroll_up: bool,
    pub scroll_down: bool,
    pub scroll_left: bool,
    pub scroll_right: bool,
    pub button8: bool,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct PointerEvent {
    pub message_type: u8,
    pub button_mask: PointerEventButtonMask,
    pub x: u16_be,
    pub y: u16_be,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct ClientCutText {
    pub message_type: u8,
    pub padding: [u8; 3],
    pub length: u32_be,
    // text: [u8; N],
}

// Server to client messages

pub const SC_MESSAGE_TYPE_FRAMEBUFFER_UPDATE: u8 = 0;
pub const SC_MESSAGE_TYPE_SET_COLOR_MAP_ENTRIES: u8 = 1;
pub const SC_MESSAGE_TYPE_BELL: u8 = 2;
pub const SC_MESSAGE_TYPE_SERVER_CUT_TEXT: u8 = 3;

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct FramebufferUpdate {
    pub message_type: u8,
    pub padding: u8,
    pub rectangle_count: u16_be,
    // rectangles: [Rectangle; N],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct Rectangle {
    pub x: u16_be,
    pub y: u16_be,
    pub width: u16_be,
    pub height: u16_be,
    pub encoding_type: u32_be,
    // data: ...
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct SetColorMapEntries {
    pub message_type: u8,
    pub padding: u8,
    pub first_color: u16_be,
    pub color_count: u16_be,
    // colors: [Color; N],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct Color {
    pub red: u16_be,
    pub green: u16_be,
    pub blue: u16_be,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct Bell {
    pub message_type: u8,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct ServerCutText {
    pub message_type: u8,
    pub padding: [u8; 3],
    pub length: u32_be,
    // text: [u8; N],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct QemuMessageHeader {
    pub message_type: u8,
    pub submessage_type: u8,
}

pub const QEMU_MESSAGE_EXTENDED_KEY_EVENT: u8 = 0;

#[repr(C)]
#[derive(Copy, Clone, Debug, AsBytes, FromBytes, FromZeroes)]
pub struct QemuExtendedKeyEvent {
    pub message_type: u8,
    pub submessage_type: u8,
    pub down_flag: u16_be,
    pub keysym: u32_be,
    pub keycode: u32_be,
}
