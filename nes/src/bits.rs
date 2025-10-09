
#[macro_export]
macro_rules! is_set {
    ($v: expr, $bit: expr) => {
        ($v & (1 << $bit)) != 0
    }
}

#[macro_export]
macro_rules! get_bit {
    ($v: expr, $bit: expr) => {
        ($v >> $bit) & 1
    }
}

#[macro_export]
macro_rules! get_bits {
    ($value: expr, $count: expr, $shift: expr) => {
        ($value >> $shift) & ((1 << $count) - 1)
    }
}

/// Example:
/// t: ...GH.. ........ <- d: ......GH
///  t = set_bit_with_mask!(self.ir.t, value as u16, 0b11, 10);
#[macro_export]
macro_rules! set_bit_with_mask {
    ($v: expr, $value: expr, $keep_mask: expr, $shift: expr) => {
        ($v & !($keep_mask << $shift)) | ((($value & $keep_mask) << $shift) & ($keep_mask << $shift))
    }
}
