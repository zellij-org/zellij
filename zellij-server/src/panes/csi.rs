/// A trait for values which have a csi-representation
pub trait Csi {
    /// Returns the csi representation of the value
    fn get_csi_str(&self) -> &str;
}
