use std::{
    fmt,
    path::{Path, PathBuf},
};

use clap::{Parser, ValueEnum};
use image;
use tracing::{debug, info, warn};
use tracing_subscriber;

mod cropping;
mod post_processing;

/// facecrop extracts crops of all faces within a given image (.png|.jpeg|.jpg)
/// or directory of images.
///
/// Crops are calculated based on the face bounding box and can be either absolute (pixels)
/// or relative to the face size (propostion of the face height to crop). Each crop is then
/// optionally resized to the given size and/or filtered out.
#[derive(Parser, Debug)]
#[command(author, version)]
#[command(
    about = "\
        facecrop extracts crops of all faces within a given image (.png|.jpeg|.jpg) \
        or directory of images.\n\n\
        Crops are calculated based on the face bounding box and can be either absolute (pixels) \
        or relative to the face size (propostion of the face height to crop). \
        Each crop is then optionally resized to the given size and/or filtered out.\
    ",
    long_about = None,
)]
struct Args {
    /// Path to the image file or directory to process
    #[arg()]
    image_path_or_dir: String,

    /// Path to write output files to
    #[arg()]
    output_dir: String,

    /// Strategy to use to crop faces. This can either be "absolute" or "relative"
    #[arg(short, long, value_enum, default_value = "relative")]
    strategy: CropStrategy,

    /// Aspect ratio (width:height) to crop the image by. 1.0 indicates a square crop
    /// while 1.5 indicates a crop that is 1.5 times as wide as it is tall.
    #[arg(short = 'a', long = "aspect_ratio", default_value = "1.0")]
    aspect_ratio: f32,

    /// Top padding. Portion of the image that should be padded on top of the face
    /// This is a float between 0.0 and 1.0
    #[arg(short, long, default_value = "0.1")]
    top_padding: f32,

    /// Portion of the image that the face should take up vertically (from the top)
    /// This is a float between 0.0 and 1.0
    #[arg(short, long, default_value = "0.3")]
    proportion_of_face: f32,

    /// Height of the crop. Used to determine the crop dimensions if strategy="absolute".
    /// If strategy="relative" and resize=true, the cropped image will be resized to this height.
    #[arg(long, default_value = "1024")]
    height: u32,

    /// Width of the crop. Used to determine the crop dimensions if strategy="absolute".
    /// If strategy="relative" and resize=true, the cropped image will be resized to this width.
    #[arg(long, default_value = "1024")]
    width: u32,

    /// True to resize the cropped image to the specified height and width. False to leave the
    /// cropped image at the original size
    #[arg(short, long, default_value = "false")]
    resize: bool,

    /// True to filter out crops that are smaller than the specified height and width. False to
    /// output all crops
    #[arg(short, long, default_value = "false")]
    filter_by_size: bool,

    /// Verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CropStrategy {
    Absolute,
    Relative,
}

impl fmt::Display for CropStrategy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
struct Paths {
    input_image_paths: Vec<PathBuf>,
    output_dir: PathBuf,
}

fn main() {
    let args = Args::parse();

    let level = match args.verbose {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing_subscriber::fmt().with_max_level(level).init();

    info!(
        "Running program with args \
        image_path_or_dir={} \
        output_dir={} \
        strategy={} \
        aspect_ratio={} \
        top_padding={} \
        proportion_of_face={} \
        height={} \
        width={} \
        resize={} \
        filter_by_size={} \
        verbose={}
        ",
        args.image_path_or_dir,
        args.output_dir,
        args.strategy,
        args.aspect_ratio,
        args.top_padding,
        args.proportion_of_face,
        args.height,
        args.width,
        args.resize,
        args.filter_by_size,
        args.verbose,
    );
    info!("Checking args");
    let paths = get_paths(&args);
    let crop_params = get_crop_params(&args);
    let post_process_params = get_post_process_params(&args);

    info!("Instantiating face detector 🤖");
    let face_detector = cropping::get_face_detector();
    info!("Starting inference and cropping 🚀");

    for image_path in &paths.input_image_paths {
        let input_image = read_image(image_path);

        let faces = cropping::detect_faces_in_image(&input_image, &*face_detector);
        debug!("Detected {} faces in {}", faces.len(), image_path.display());

        let image_name = image_path.file_stem().unwrap().to_str().unwrap();

        process_faces(
            cropping::CropInputs {
                input_image: &input_image,
                faces: &faces,
            },
            &crop_params,
            &post_process_params,
            &paths.output_dir,
            image_name,
        );
    }
    info!("Finished processing images 🎉");
}

fn get_paths(args: &Args) -> Paths {
    let input_image_path = std::path::PathBuf::from(&args.image_path_or_dir);
    if !input_image_path.exists() {
        panic!("Input path does not exist");
    }
    let input_image_paths = match input_image_path.is_file() {
        true => {
            info!("Received file {}", input_image_path.display());
            vec![input_image_path.clone()]
        }
        false => {
            info!("Received directory {}", input_image_path.display());

            let mut input_image_paths = vec![];
            for entry in std::fs::read_dir(&input_image_path)
                .unwrap_or_else(|_| panic!("Failed to read input directory"))
            {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "jpg" || extension == "jpeg" || extension == "png" {
                            debug!("Found image {}", path.display());
                            input_image_paths.push(path);
                        }
                    }
                }
            }
            input_image_paths
        }
    };

    let output_dir = std::path::PathBuf::from(&args.output_dir);
    if output_dir.exists() && !output_dir.is_dir() {
        panic!("Output directory is not a directory");
    }
    std::fs::create_dir_all(&args.output_dir)
        .unwrap_or_else(|_| panic!("Failed to create output directory"));

    Paths {
        input_image_paths: input_image_paths,
        output_dir: output_dir,
    }
}

fn get_crop_params(args: &Args) -> cropping::CropParams {
    if args.top_padding < 0.0 || args.top_padding > 1.0 {
        panic!("Top padding must be between 0.0 and 1.0");
    }
    let crop_params_kind = match args.strategy {
        CropStrategy::Absolute => cropping::CropParamsKind::Absolute(cropping::AbsoluteCrop {
            height: args.height,
            width: args.width,
        }),
        CropStrategy::Relative => {
            if args.aspect_ratio <= 0.0 {
                panic!("Aspect ratio must be greater than 0");
            }
            if args.proportion_of_face < 0.0 || args.proportion_of_face > 1.0 {
                panic!("Proportion of face must be between 0.0 and 1.0");
            }

            cropping::CropParamsKind::Relative(cropping::RelativeCrop {
                aspect_ratio: args.aspect_ratio,
                proportion_of_face: args.proportion_of_face,
            })
        }
    };

    cropping::CropParams {
        top_padding: args.top_padding,
        kind: crop_params_kind,
    }
}

fn get_post_process_params(args: &Args) -> post_processing::PostProcessParams {
    post_processing::PostProcessParams {
        resize: args.resize,
        filter_by_size: args.filter_by_size,
        height: args.height,
        width: args.width,
    }
}

fn read_image(input_image_path: &std::path::Path) -> image::RgbImage {
    let input_image = image::open(&input_image_path.to_str().unwrap())
        .unwrap_or_else(|_| panic!("Failed to open image file"))
        .into_rgb8();

    input_image
}

fn process_faces(
    faces_to_crop: cropping::CropInputs,
    crop_params: &cropping::CropParams,
    post_process_params: &post_processing::PostProcessParams,
    output_dir: &Path,
    image_name: &str,
) {
    let crop_outputs = cropping::crop_faces(faces_to_crop, crop_params);
    if crop_outputs.is_none() {
        warn!("No crops for image {}. Skipping", image_name);
        return;
    }

    for (i, crop) in crop_outputs.unwrap().iter().enumerate() {
        let output_image = post_processing::post_process_image(&crop.image, post_process_params);
        match output_image {
            Some(cropped_image) => {
                let output_path =
                    output_dir.join(format!("{}-{}-{:.3}.jpg", image_name, i, crop.confidence));
                cropped_image
                    .save(&output_path)
                    .unwrap_or_else(|_| panic!("Failed to save output image"));
                info!(
                    "Saved face {} in image {} to {}",
                    i,
                    image_name,
                    output_path.display()
                );
            }
            None => warn!("Cropped image is too small. Skipping"),
        }
    }
}
