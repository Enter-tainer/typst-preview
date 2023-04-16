#[tracing::instrument(skip_all)]
pub fn rle_encode<const CHUNK_LEN: usize>(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() / 16);
    assert_eq!(input.len() % 4, 0, "Input must be a multiple of 4 bytes");
    assert!(input.len() >= 8, "Input must be at least 8 bytes long");
    let mut last_chunk: &[u8; CHUNK_LEN] = input[0..CHUNK_LEN].try_into().unwrap();
    let chunk_size = 64;
    let mut count: u8 = 0;
    let iter = input.chunks_exact(chunk_size);
    for big_chunk in iter.clone() {
        for chunk in big_chunk.chunks_exact(CHUNK_LEN) {
            let current_chunk: &[u8; CHUNK_LEN] = chunk.try_into().unwrap();
            if last_chunk == current_chunk {
                if count == u8::MAX {
                    output.extend(&count.to_le_bytes());
                    output.extend(last_chunk);
                    count = 0;
                }
                count += 1;
            } else {
                output.extend(&count.to_le_bytes());
                output.extend(last_chunk);
                count = 1;
                last_chunk = current_chunk;
            }
        }
    }

    for chunk in iter.remainder().chunks_exact(CHUNK_LEN) {
        let current_chunk: &[u8; CHUNK_LEN] = chunk.try_into().unwrap();
        if last_chunk == current_chunk {
            if count == u8::MAX {
                output.extend(&count.to_le_bytes());
                output.extend(last_chunk);
                count = 0;
            }
            count += 1;
        } else {
            output.extend(&count.to_le_bytes());
            output.extend(last_chunk);
            count = 1;
            last_chunk = current_chunk;
        }
    }
    output.extend(&count.to_le_bytes());
    output.extend(last_chunk);
    output
}

mod test {

    #[test]
    fn test_rle_encode() {
        // assert_eq!(rle_encode::<4>(&[0, 0, 0, 0]), &[4, 0, 0, 0, 0]);
        assert_eq!(rle_encode::<4>(&[2, 3, 3, 3, 2, 3, 3, 3]), &[2, 2, 3, 3, 3]);
        assert_eq!(
            rle_encode::<4>(&[2, 3, 3, 3, 2, 3, 3, 3, 1, 1, 4, 5]),
            &[2, 2, 3, 3, 3, 1, 1, 1, 4, 5]
        );
    }
}
