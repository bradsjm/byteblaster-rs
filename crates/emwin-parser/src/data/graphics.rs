use super::{NonTextProductMeta, container_from_ext, goes_re, imgmod_re, radar_re};

pub(super) fn detect_graphics(filename_upper: &str) -> Option<NonTextProductMeta> {
    if let Some(caps) = radar_re().captures(filename_upper) {
        let code = caps.get(1).expect("code group exists").as_str().to_string();
        return Some(NonTextProductMeta {
            family: "radar_graphic",
            title: "Radar graphic",
            code,
            container: "raw",
            pil: None,
            wmo_prefix: None,
        });
    }

    if let Some(caps) = goes_re().captures(filename_upper) {
        let code = caps.get(1).expect("code group exists").as_str().to_string();
        let ext = caps.get(2).expect("ext group exists").as_str();
        return Some(NonTextProductMeta {
            family: "goes_graphic",
            title: "GOES satellite graphic",
            code,
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
        });
    }

    if let Some(caps) = imgmod_re().captures(filename_upper) {
        let code = caps.get(1).expect("code group exists").as_str().to_string();
        let ext = caps.get(2).expect("ext group exists").as_str();
        return Some(NonTextProductMeta {
            family: "nws_graphic",
            title: "NWS graphic product",
            code,
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
        });
    }

    None
}
