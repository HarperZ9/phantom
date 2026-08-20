#[cfg_attr(not(windows), allow(dead_code))]
pub const ICON_SIZE: u32 = 32;

#[cfg_attr(not(windows), allow(dead_code))]
pub const GREEN: (u8, u8, u8) = (61, 214, 140);
#[cfg_attr(not(windows), allow(dead_code))]
pub const GREY: (u8, u8, u8) = (140, 140, 150);
#[cfg_attr(not(windows), allow(dead_code))]
pub const AMBER: (u8, u8, u8) = (232, 164, 69);

#[cfg_attr(not(windows), allow(dead_code))]
pub fn shield_rgba(r: u8, g: u8, b: u8) -> Vec<u8> {
    let s = ICON_SIZE;
    let mut buf = vec![0u8; (s * s * 4) as usize];
    let cx = s as f32 / 2.0;

    for py in 0..s {
        for px in 0..s {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;

            let top = s as f32 * 0.06;
            let bot = s as f32 * 0.94;
            let left = s as f32 * 0.12;
            let right = s as f32 * 0.88;
            let mid = s as f32 * 0.50;

            let inside = if y < top || y > bot {
                false
            } else if y <= mid {
                x >= left && x <= right
            } else {
                let t = (y - mid) / (bot - mid);
                let hw = (right - left) / 2.0 * (1.0 - t);
                (x - cx).abs() <= hw
            };

            if inside {
                let i = ((py * s + px) * 4) as usize;
                buf[i] = r;
                buf[i + 1] = g;
                buf[i + 2] = b;
                buf[i + 3] = 255;
            }
        }
    }
    buf
}

#[cfg(windows)]
pub unsafe fn create_hicon(rgba: &[u8]) -> isize {
    extern "system" {
        fn CreateCompatibleDC(hdc: isize) -> isize;
        fn DeleteDC(hdc: isize) -> i32;
        fn CreateDIBSection(
            hdc: isize,
            pbmi: *const u8,
            usage: u32,
            ppv_bits: *mut *mut u8,
            h_section: isize,
            offset: u32,
        ) -> isize;
        fn CreateBitmap(w: i32, h: i32, planes: u32, bpp: u32, bits: *const u8) -> isize;
        fn CreateIconIndirect(info: *mut IconInfo) -> isize;
        fn DeleteObject(obj: isize) -> i32;
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }

    #[repr(C)]
    struct IconInfo {
        f_icon: i32,
        x_hotspot: u32,
        y_hotspot: u32,
        hbm_mask: isize,
        hbm_color: isize,
    }

    let s = ICON_SIZE as i32;
    let pixel_count = (ICON_SIZE * ICON_SIZE) as usize;

    let hdc = CreateCompatibleDC(0);
    let header = BitmapInfoHeader {
        size: 40,
        width: s,
        height: -s, // top-down
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_ppm: 0,
        y_ppm: 0,
        clr_used: 0,
        clr_important: 0,
    };

    let mut bits_ptr: *mut u8 = std::ptr::null_mut();
    let hbm_color = CreateDIBSection(
        hdc,
        &header as *const _ as *const u8,
        0,
        &mut bits_ptr,
        0,
        0,
    );
    DeleteDC(hdc);

    if hbm_color == 0 || bits_ptr.is_null() {
        return 0;
    }

    let dst = std::slice::from_raw_parts_mut(bits_ptr, pixel_count * 4);
    for i in 0..pixel_count {
        let si = i * 4;
        let a = rgba[si + 3] as u32;
        dst[si] = ((rgba[si + 2] as u32 * a) / 255) as u8;
        dst[si + 1] = ((rgba[si + 1] as u32 * a) / 255) as u8;
        dst[si + 2] = ((rgba[si] as u32 * a) / 255) as u8;
        dst[si + 3] = rgba[si + 3];
    }

    let mask_stride = ((ICON_SIZE + 31) / 32 * 4) as usize;
    let mask = vec![0u8; mask_stride * ICON_SIZE as usize];
    let hbm_mask = CreateBitmap(s, s, 1, 1, mask.as_ptr());

    let mut info = IconInfo {
        f_icon: 1,
        x_hotspot: 0,
        y_hotspot: 0,
        hbm_mask,
        hbm_color,
    };
    let icon = CreateIconIndirect(&mut info);

    DeleteObject(hbm_color);
    DeleteObject(hbm_mask);
    icon
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shield_correct_buffer_size() {
        let px = shield_rgba(255, 0, 0);
        assert_eq!(px.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn shield_transparent_corners() {
        let px = shield_rgba(255, 0, 0);
        assert_eq!(px[3], 0);
        let last = ((ICON_SIZE * ICON_SIZE - 1) * 4 + 3) as usize;
        assert_eq!(px[last], 0);
    }

    #[test]
    fn shield_opaque_center() {
        let px = shield_rgba(255, 0, 0);
        let c = ICON_SIZE / 2;
        let i = ((c * ICON_SIZE + c) * 4 + 3) as usize;
        assert_eq!(px[i], 255);
    }

    #[test]
    fn all_three_colors_distinct() {
        let g = shield_rgba(GREEN.0, GREEN.1, GREEN.2);
        let r = shield_rgba(GREY.0, GREY.1, GREY.2);
        let a = shield_rgba(AMBER.0, AMBER.1, AMBER.2);
        let c = ICON_SIZE / 2;
        let ci = ((c * ICON_SIZE + c) * 4) as usize;
        assert_ne!(&g[ci..ci + 3], &r[ci..ci + 3]);
        assert_ne!(&g[ci..ci + 3], &a[ci..ci + 3]);
        assert_ne!(&r[ci..ci + 3], &a[ci..ci + 3]);
    }

    #[test]
    fn shield_top_center_is_filled() {
        let px = shield_rgba(100, 200, 50);
        let top_center = ((3 * ICON_SIZE + ICON_SIZE / 2) * 4) as usize;
        assert_eq!(px[top_center + 3], 255);
    }
}
