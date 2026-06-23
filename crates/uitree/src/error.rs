/// Errors that can occur during UI tree construction and traversal.
#[derive(Debug, thiserror::Error)]
pub enum UITreeError {
    /// A Windows UI Automation COM call failed.
    #[error("UI Automation error: {0}")]
    UIAutomation(String),

    /// Failed to receive a result from a background thread channel.
    #[error("Channel receive error: {0}")]
    ChannelRecv(String),

    /// XML serialization or parsing failed.
    #[error("XML error: {0}")]
    XmlError(String),

    /// Could not create a UIAutomation instance (COM may not be initialized).
    #[error("UIAutomation instance creation failed")]
    NoUIAutomation,

    /// Tree construction was cancelled (e.g. due to timeout on the receiver side).
    #[error("Tree construction cancelled")]
    Cancelled,
}
