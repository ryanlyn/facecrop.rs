use image;

#[derive(Debug)]
pub struct PostProcessParams {
    pub resize: bool,
    pub filter_by_size: bool,
    pub height: u32,
    pub width: u32,
}

pub fn post_process_image(
    input_image: &image::RgbImage,
    post_process_params: &PostProcessParams,
) -> Option<image::RgbImage> {
    if post_process_params.filter_by_size {
        if input_image.width() < post_process_params.width
            || input_image.height() < post_process_params.height
        {
            return None;
        }
    }

    let resized_image = match post_process_params.resize {
        true => image::imageops::resize(
            input_image,
            post_process_params.width,
            post_process_params.height,
            image::imageops::FilterType::Lanczos3,
        ),
        false => input_image.clone(),
    };

    Some(resized_image)
}
