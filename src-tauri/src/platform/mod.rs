//! OS integration: clipboard monitoring, clipboard IO, focus restore and
//! synthetic key input. Every platform exposes the same free functions;
//! only Windows is implemented for now.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub use self::unsupported::*;
