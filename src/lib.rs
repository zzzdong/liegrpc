pub mod client;
mod codec;
pub mod grpc;
pub mod metadata;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use bytes::{BufMut, BytesMut};

    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }

    #[test]
    fn test_bytes_chunk_mut() {
        let mut buf = BytesMut::new();

        buf.put_u8(0);
        buf.put_u32(0);

        println!("{:?}", &buf);

        buf.put_u32(1);

        println!("{:?}", &buf);

        buf.chunk_mut()[1..5].copy_from_slice(&(0xFF as u32).to_be_bytes());

        println!("{:?}", &buf);
    }
}
