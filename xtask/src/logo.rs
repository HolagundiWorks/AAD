//! `cargo xtask logo`: draw the AAD logo and write every icon the project ships.
//!
//! The logo is straight lines, so it is drawn here directly (no image tools, no fonts): a blue
//! square, a white "A" mark, and the white cursor crosshair and pick box, which are drawn in the
//! opposite colour where they cross the mark. Everything under `assets/app-icon/` is generated:
//!
//! - `aad-logo.svg`: the vector source
//! - `aad-1024.png`, `-256.png`, `-64.png`: the square icons
//! - `aad-macos-512.png`: the rounded icon with a margin, for macOS
//! - `hicolor/<n>x<n>/apps/io.github.holagundiworks.aad.png`: the Linux sizes
//! - `aad.ico` (Windows) and `aad.icns` (macOS)
//!
//! Coordinates are those of the original 1254 x 1254 artwork. The PNG writer is a small deflate
//! (fixed Huffman codes with greedy matching): the artwork is flat colour, so files stay small.

use std::path::Path;

const ART: f64 = 1254.0;
const BLUE: [f64; 3] = [0.0, 102.0, 255.0];
const WHITE: [f64; 3] = [255.0, 255.0, 255.0];
/// The mark, clockwise from the top left of its top bar.
const MARK: [(f64, f64); 8] =
    [(740.0, 261.0), (934.0, 261.0), (934.0, 1043.0), (725.0, 1043.0), (725.0, 541.0), (210.0, 1043.0), (210.0, 783.0), (740.0, 261.0)];
const CROSS_X: f64 = 897.5;
const CROSS_Y: f64 = 342.0;
const BOX: (f64, f64, f64, f64) = (872.0, 315.0, 925.0, 369.0);
const STROKE: f64 = 1.6;

pub const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1254 1254" width="1254" height="1254">
  <title>AAD - Accelerated Architecture Drafting</title>
  <defs>
    <clipPath id="mark"><path d="M740 261H934V1043H725V541L210 1043V783Z"/></clipPath>
  </defs>
  <rect width="1254" height="1254" fill="#0066ff"/>
  <path d="M740 261H934V1043H725V541L210 1043V783Z" fill="#fff"/>
  <g stroke="#fff" stroke-width="3.2" fill="none">
    <line x1="0" y1="342" x2="1254" y2="342"/>
    <line x1="897.5" y1="0" x2="897.5" y2="1254"/>
  </g>
  <g clip-path="url(#mark)" stroke="#0066ff" stroke-width="3.2" fill="none">
    <line x1="0" y1="342" x2="1254" y2="342"/>
    <line x1="897.5" y1="0" x2="897.5" y2="1254"/>
    <rect x="872" y="315" width="53" height="54"/>
  </g>
</svg>
"##;

fn in_mark(x: f64, y: f64) -> bool {
    let mut inside = false;
    let n = MARK.len() - 1;
    for i in 0..n {
        let ((x1, y1), (x2, y2)) = (MARK[i], MARK[i + 1]);
        if (y1 > y) != (y2 > y) && x < x1 + (y - y1) * (x2 - x1) / (y2 - y1) {
            inside = !inside;
        }
    }
    inside
}

fn on_crosshair(x: f64, y: f64) -> bool {
    let (bx0, by0, bx1, by1) = BOX;
    let line = (x - CROSS_X).abs() <= STROKE || (y - CROSS_Y).abs() <= STROKE;
    let outer = x >= bx0 - STROKE && x <= bx1 + STROKE && y >= by0 - STROKE && y <= by1 + STROKE;
    let inner = x > bx0 + STROKE && x < bx1 - STROKE && y > by0 + STROKE && y < by1 - STROKE;
    line || (outer && !inner)
}

/// The colour of artwork point (x, y): blue or white.
fn art(x: f64, y: f64, crosshair: bool) -> [f64; 3] {
    let white = in_mark(x, y) ^ (crosshair && on_crosshair(x, y));
    if white { WHITE } else { BLUE }
}

/// One icon as RGBA. `mac`: the artwork sits in a rounded square with a margin on a transparent
/// ground, as macOS draws icons; otherwise it fills the whole square.
fn render(size: u32, mac: bool) -> Vec<u8> {
    const SS: u32 = 4;
    let s = f64::from(size);
    let (inner, off) = if mac { (s * 0.8125, s * 0.09375) } else { (s, 0.0) };
    let radius = if mac { inner * 0.2237 } else { 0.0 };
    // The crosshair is too fine to read in a small icon.
    let crosshair = size >= 128;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut acc, mut alpha) = ([0.0f64; 3], 0.0f64);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = f64::from(px) + (f64::from(sx) + 0.5) / f64::from(SS);
                    let y = f64::from(py) + (f64::from(sy) + 0.5) / f64::from(SS);
                    let (u, v) = (x - off, y - off);
                    if u < 0.0 || v < 0.0 || u > inner || v > inner {
                        continue;
                    }
                    if radius > 0.0 {
                        let cx = u.clamp(radius, inner - radius);
                        let cy = v.clamp(radius, inner - radius);
                        if (u - cx).hypot(v - cy) > radius {
                            continue;
                        }
                    }
                    let c = art(u / inner * ART, v / inner * ART, crosshair);
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                    alpha += 1.0;
                }
            }
            let n = f64::from(SS * SS);
            if alpha == 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.extend_from_slice(&[
                    (acc[0] / alpha).round() as u8,
                    (acc[1] / alpha).round() as u8,
                    (acc[2] / alpha).round() as u8,
                    (alpha / n * 255.0).round() as u8,
                ]);
            }
        }
    }
    out
}

// ---------------- PNG ----------------

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, t) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = 0xffff_ffffu32;
    for b in data {
        c = table[((c ^ u32::from(*b)) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for x in chunk {
            a += u32::from(*x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, bits: u32) {
        self.acc |= v << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push((self.acc & 0xff) as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// A Huffman code: packed most significant bit first.
    fn code(&mut self, code: u32, bits: u32) {
        let mut r = 0;
        for i in 0..bits {
            r |= ((code >> i) & 1) << (bits - 1 - i);
        }
        self.put(r, bits);
    }
    fn literal(&mut self, v: u32) {
        match v {
            0..=143 => self.code(0x30 + v, 8),
            144..=255 => self.code(0x190 + v - 144, 9),
            256..=279 => self.code(v - 256, 7),
            _ => self.code(0xc0 + v - 280, 8),
        }
    }
}

const LEN_BASE: [u32; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u32; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// zlib stream: one fixed-Huffman deflate block with greedy LZ77 matching.
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: vec![0x78, 0x01], acc: 0, n: 0 };
    b.put(1, 1);
    b.put(1, 2);
    const HASH: usize = 1 << 15;
    let mut head = vec![u32::MAX; HASH];
    let mut prev = vec![u32::MAX; data.len()];
    let hash = |i: usize| -> usize { ((u32::from(data[i]) << 10) ^ (u32::from(data[i + 1]) << 5) ^ u32::from(data[i + 2])) as usize & (HASH - 1) };
    let mut i = 0;
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0usize, 0usize);
        if i + 3 <= data.len() {
            let h = hash(i);
            let mut cand = head[h];
            let mut steps = 0;
            while cand != u32::MAX && steps < 24 {
                let c = cand as usize;
                let dist = i - c;
                if dist > 32768 {
                    break;
                }
                let max = (data.len() - i).min(258);
                let mut l = 0;
                while l < max && data[c + l] == data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = dist;
                    if l == max {
                        break;
                    }
                }
                cand = prev[c];
                steps += 1;
            }
        }
        let advance = if best_len >= 3 {
            let li = LEN_BASE.iter().rposition(|&x| x <= best_len as u32).unwrap_or(0);
            b.literal(257 + li as u32);
            b.put(best_len as u32 - LEN_BASE[li], LEN_EXTRA[li]);
            let di = DIST_BASE.iter().rposition(|&x| x <= best_dist as u32).unwrap_or(0);
            b.code(di as u32, 5);
            b.put(best_dist as u32 - DIST_BASE[di], DIST_EXTRA[di]);
            best_len
        } else {
            b.literal(u32::from(data[i]));
            1
        };
        for (k, slot) in prev.iter_mut().enumerate().skip(i).take(advance) {
            if k + 3 <= data.len() {
                let h = hash(k);
                *slot = head[h];
                head[h] = k as u32;
            }
        }
        i += advance;
    }
    b.literal(256);
    if b.n > 0 {
        b.out.push(b.acc as u8);
    }
    b.out.extend_from_slice(&adler32(data).to_be_bytes());
    b.out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

pub fn png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + size as usize);
    for row in rgba.chunks((size * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// An `.icns` file of PNG images: (OSType, PNG bytes).
fn icns(parts: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let total: usize = 8 + parts.iter().map(|(_, d)| 8 + d.len()).sum::<usize>();
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(total as u32).to_be_bytes());
    for (kind, data) in parts {
        out.extend_from_slice(*kind);
        out.extend_from_slice(&((8 + data.len()) as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn run(root: &Path) -> Result<(), String> {
    let dir = root.join("assets/app-icon");
    let id = "io.github.holagundiworks.aad";
    write(&dir.join("aad-logo.svg"), SVG.as_bytes())?;
    let square = |n: u32| png(n, &render(n, false));
    let mac = |n: u32| png(n, &render(n, true));
    for n in [64u32, 256, 1024] {
        write(&dir.join(format!("aad-{n}.png")), &square(n))?;
    }
    write(&dir.join("aad-macos-512.png"), &mac(512))?;
    for n in [16u32, 24, 32, 48, 64, 128, 256, 512] {
        write(&dir.join(format!("hicolor/{n}x{n}/apps/{id}.png")), &square(n))?;
    }
    let pngs: Vec<Vec<u8>> = [16u32, 20, 24, 32, 40, 48, 64, 128, 256].iter().map(|n| square(*n)).collect();
    write(&dir.join("aad.ico"), &crate::ico::pack(&pngs)?)?;
    let icns_bytes = icns(&[
        (b"icp4", mac(16)),
        (b"icp5", mac(32)),
        (b"icp6", mac(64)),
        (b"ic07", mac(128)),
        (b"ic08", mac(256)),
        (b"ic09", mac(512)),
        (b"ic10", mac(1024)),
    ]);
    write(&dir.join("aad.icns"), &icns_bytes)?;
    println!("logo written to {}", dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_and_crosshair_follow_the_artwork() {
        assert!(in_mark(800.0, 700.0), "the stem");
        assert!(in_mark(400.0, 800.0), "the arm");
        assert!(!in_mark(100.0, 100.0) && !in_mark(500.0, 400.0), "outside");
        assert!(!in_mark(650.0, 900.0), "the notch under the arm");
        // Blue ground, white mark; the crosshair is white on the ground and blue on the mark.
        assert_eq!(art(100.0, 100.0, true), BLUE);
        assert_eq!(art(800.0, 700.0, true), WHITE);
        assert_eq!(art(CROSS_X, 100.0, true), WHITE);
        assert_eq!(art(CROSS_X, 700.0, true), BLUE);
        assert_eq!(art(CROSS_X, 700.0, false), WHITE);
    }

    #[test]
    fn png_output_is_a_valid_deflate_stream_that_ico_accepts() {
        let rgba = render(64, false);
        let p = png(64, &rgba);
        assert_eq!(crate::ico::png_size(&p), Ok((64, 64)));
        assert!(p.len() < 2000, "flat artwork compresses: {} bytes", p.len());
        // The stored adler32 matches the data the stream decodes to: decode it with a tiny inflater.
        assert_eq!(inflate_fixed(&p), expected_raw(&rgba, 64));
        let ico = crate::ico::pack(&[p]).unwrap();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
    }

    fn expected_raw(rgba: &[u8], size: u32) -> Vec<u8> {
        let mut raw = Vec::new();
        for row in rgba.chunks((size * 4) as usize) {
            raw.push(0);
            raw.extend_from_slice(row);
        }
        raw
    }

    /// Decode the first IDAT of a PNG written by `png` (fixed-Huffman deflate only).
    fn inflate_fixed(png: &[u8]) -> Vec<u8> {
        let mut i = 8;
        let mut idat = Vec::new();
        while i + 8 <= png.len() {
            let len = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
            if &png[i + 4..i + 8] == b"IDAT" {
                idat = png[i + 8..i + 8 + len].to_vec();
            }
            i += 12 + len;
        }
        let data = &idat[2..idat.len() - 4];
        let (mut pos, mut out) = (0usize, Vec::new());
        let bit = |pos: &mut usize| -> u32 {
            let v = (data[*pos / 8] >> (*pos % 8)) & 1;
            *pos += 1;
            u32::from(v)
        };
        let bits = |pos: &mut usize, n: u32| -> u32 {
            let mut v = 0;
            for k in 0..n {
                v |= bit(pos) << k;
            }
            v
        };
        assert_eq!(bits(&mut pos, 3), 0b011, "final block, fixed codes");
        loop {
            // Read a literal/length code, most significant bit first.
            let mut code = 0u32;
            let mut len = 0;
            let sym = loop {
                code = (code << 1) | bit(&mut pos);
                len += 1;
                match len {
                    7 if code <= 0x17 => break 256 + code,
                    8 if (0x30..=0xbf).contains(&code) => break code - 0x30,
                    8 if (0xc0..=0xc7).contains(&code) => break 280 + code - 0xc0,
                    9 if (0x190..=0x1ff).contains(&code) => break 144 + code - 0x190,
                    _ => {}
                }
                assert!(len < 10, "bad code");
            };
            match sym {
                0..=255 => out.push(sym as u8),
                256 => break,
                _ => {
                    let li = (sym - 257) as usize;
                    let l = (LEN_BASE[li] + bits(&mut pos, LEN_EXTRA[li])) as usize;
                    let mut d = 0u32;
                    for _ in 0..5 {
                        d = (d << 1) | bit(&mut pos);
                    }
                    let dist = (DIST_BASE[d as usize] + bits(&mut pos, DIST_EXTRA[d as usize])) as usize;
                    for _ in 0..l {
                        out.push(out[out.len() - dist]);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn the_icns_header_covers_every_part() {
        let parts = [(b"ic07", vec![1u8, 2, 3]), (b"ic08", vec![4u8; 10])];
        let f = icns(&parts);
        assert_eq!(&f[..4], b"icns");
        assert_eq!(u32::from_be_bytes([f[4], f[5], f[6], f[7]]) as usize, f.len());
        assert_eq!(f.len(), 8 + 8 + 3 + 8 + 10);
    }
}
