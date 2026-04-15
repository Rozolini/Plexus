mod channel;
mod process;

pub use channel::{Consumer, ErrorClass, IpcError, Producer, SharedChannel, SharedEndpoint};
pub use process::{PeerRole, ProcessIdentity};
