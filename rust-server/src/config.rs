pub(crate) const PORT: u16 = 3000;
pub(crate) const TLS_CONTENT_TYPE_HANDSHAKE: u8 = 0x16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_is_3000() {
        assert_eq!(PORT, 3000);
    }

    #[test]
    fn tls_handshake_byte_is_0x16() {
        assert_eq!(TLS_CONTENT_TYPE_HANDSHAKE, 0x16);
    }
}
