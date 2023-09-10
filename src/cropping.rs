use rust_faces::{
    BlazeFaceParams, Face, FaceDetection, FaceDetector, FaceDetectorBuilder, InferParams, Rect,
    ToArray3,
};

#[derive(Debug)]
pub struct CropInputs<'a> {
    pub input_image: &'a image::RgbImage,
    pub faces: &'a Vec<Face>,
}

#[derive(Debug)]
pub struct CropOutputs {
    pub image: image::RgbImage,
    pub confidence: f32,
}

#[derive(Debug)]
pub struct CropParams {
    pub top_padding: f32,
    pub kind: CropParamsKind,
}

#[derive(Debug)]
pub enum CropParamsKind {
    Absolute(AbsoluteCrop),
    Relative(RelativeCrop),
}

#[derive(Debug)]
pub struct AbsoluteCrop {
    pub height: u32,
    pub width: u32,
}

#[derive(Debug)]
pub struct RelativeCrop {
    pub aspect_ratio: f32,
    pub proportion_of_face: f32,
}

pub fn get_face_detector() -> Box<dyn FaceDetector> {
    let face_detector =
        FaceDetectorBuilder::new(FaceDetection::BlazeFace640(BlazeFaceParams::default()))
            .download()
            .infer_params(InferParams::default())
            .build()
            .unwrap_or_else(|_| panic!("Failed to build face detector"));
    face_detector
}

pub fn detect_faces_in_image(
    input_image: &image::RgbImage,
    face_detector: &dyn FaceDetector,
) -> Vec<Face> {
    let preprocessed_image = input_image.clone().into_array3();

    let faces = face_detector
        .detect(preprocessed_image.view().into_dyn())
        .unwrap_or_else(|_| panic!("Failed to detect faces"));

    faces
}

pub fn crop_faces(faces_to_crop: CropInputs, crop_params: &CropParams) -> Option<Vec<CropOutputs>> {
    if faces_to_crop.faces.len() == 0 {
        return None;
    }

    let mut outputs = Vec::new();
    for face in faces_to_crop.faces.iter() {
        let crop = calculate_face_crop(
            &face.rect,
            &Rect::at(0.0, 0.0).with_size(
                faces_to_crop.input_image.width() as f32,
                faces_to_crop.input_image.height() as f32,
            ),
            &crop_params,
        );
        let cropped_image = image::imageops::crop_imm(
            faces_to_crop.input_image,
            crop.x as u32,
            crop.y as u32,
            crop.width as u32,
            crop.height as u32,
        )
        .to_image();

        outputs.push(CropOutputs {
            image: cropped_image,
            confidence: face.confidence,
        });
    }

    Some(outputs)
}

fn calculate_face_crop(face: &Rect, image: &Rect, params: &CropParams) -> Rect {
    let (crop_height, crop_width) = match &params.kind {
        CropParamsKind::Absolute(absolute_params) => {
            (absolute_params.height as f32, absolute_params.width as f32)
        }
        CropParamsKind::Relative(relative_params) => calculate_crop_dimensions_by_ratios(
            face,
            relative_params.aspect_ratio,
            relative_params.proportion_of_face,
        ),
    };

    let (crop_x, crop_y) =
        calculate_crop_position(face, crop_height, crop_width, params.top_padding);

    Rect::at(crop_x, crop_y)
        .with_size(crop_width, crop_height)
        .intersection(image) // can never crop outside the image
}

/// Function to calculate the crop dimensions (height and width) for a detected face in an image.
///
/// # Arguments
///
/// * `face` - The dimensions of the face.
/// * `aspect_ratio` - The desired width-to-height ratio of the crop.
/// * `proportion_of_face` - The proportion of the image height that the face should take up.
///
/// # Returns
///
/// * A tuple (crop_height, crop_width) representing the crop dimensions.
fn calculate_crop_dimensions_by_ratios(
    face: &Rect,
    aspect_ratio: f32,
    proportion_of_face: f32,
) -> (f32, f32) {
    let crop_height = face.height / proportion_of_face;
    let crop_width = crop_height * aspect_ratio;
    (crop_height, crop_width)
}

/// Function to calculate the crop position (x and y) for a detected face in an image.
///
/// # Arguments
///
/// * `face` - The dimensions of the face.
/// * `crop_height` - The calculated height of the crop.
/// * `crop_width` - The calculated width of the crop.
/// * `top_padding` - The proportion of the image that should be above the face.
///
/// # Returns
///
/// * A tuple (crop_x, crop_y) representing the crop position.
fn calculate_crop_position(
    face: &Rect,
    crop_height: f32,
    crop_width: f32,
    top_padding: f32,
) -> (f32, f32) {
    let crop_x = face.x + (face.width / 2.0) - (crop_width / 2.0);
    let crop_y = face.y - (crop_height * top_padding);
    (crop_x, crop_y)
}
