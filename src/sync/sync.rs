use dpcsync::{api, chunker, CompressionAlgorithm};
use std::path::Path;
use tokio::fs::File;

pub async fn compress_archive<T: AsRef<Path>>(
    input: T,
    output: T,
    chunker_config: chunker::Config,
    algorithm: Option<CompressionAlgorithm>,
) {
    let compression = algorithm.map(|compression_algorithm| {
        dpcsync::Compression::try_new(compression_algorithm, compression_algorithm.max_level())
            .unwrap()
    });

    let options = dpcsync::api::compress::CreateArchiveOptions {
        chunker_config,
        compression,
        ..Default::default()
    };
    let mut input_file = File::from_std(std::fs::File::open(input).unwrap());
    let force_create = true;
    let mut output_file = File::from_std(
        std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create(force_create)
            .truncate(force_create)
            .create_new(!force_create)
            .open(output)
            .unwrap(),
    );
    api::compress::create_archive(&mut input_file, &mut output_file, &options)
        .await
        .unwrap();
}
