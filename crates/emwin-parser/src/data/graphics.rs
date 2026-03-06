use super::{NonTextProductMeta, container_from_ext, goes_re, imgmod_re, radar_re};

pub(super) fn detect_graphics(filename_upper: &str) -> Option<NonTextProductMeta> {
    if radar_re().captures(filename_upper).is_some() {
        return Some(NonTextProductMeta {
            family: "radar_graphic",
            title: "Radar graphic",
            container: "raw",
            pil: None,
            wmo_prefix: None,
        });
    }

    if let Some(caps) = goes_re().captures(filename_upper) {
        let ext = caps.get(2).expect("ext group exists").as_str();
        return Some(NonTextProductMeta {
            family: "goes_graphic",
            title: "GOES satellite graphic",
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
        });
    }

    if let Some(caps) = imgmod_re().captures(filename_upper) {
        let ext = caps.get(2).expect("ext group exists").as_str();
        return Some(NonTextProductMeta {
            family: "nws_graphic",
            title: "NWS graphic product",
            container: container_from_ext(ext),
            pil: None,
            wmo_prefix: None,
        });
    }

    None
}
