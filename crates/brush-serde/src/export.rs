use std::io::Write;

use brush_render::gaussian_splats::Splats;
use brush_render::sh::sh_coeffs_for_degree;
use burn::tensor::Transaction;
use glam::Vec3;
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Serialize, Serializer};
use serde_ply::{SerializeError, SerializeOptions};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Failed to fetch splat data from GPU")]
    FetchFailed,
    #[error("Failed to convert tensor data to f32 - data may be corrupted")]
    DataConversion,
    #[error("PLY serialization failed: {0}")]
    Serialize(#[from] SerializeError),
}

const SH_NAMES: [&str; 72] = brush_serde_macros::sh_field_names!();

/// Splat data read back from the GPU as flat arrays, ready to be written as a PLY.
///
/// Rows are generated on the fly while writing, so exporting never builds a
/// per-splat object list or (unless asked for) a whole-file byte buffer.
pub struct PlyExport {
    /// `[n, 10]`: means(3) + rotations(4) + log_scales(3).
    transforms: Vec<f32>,
    raw_opacities: Vec<f32>,
    /// `[n, channel, coeffs]` (inria layout).
    sh_coeffs: Vec<f32>,
    coeffs_per_channel: usize,
    num_splats: usize,
    comments: Vec<String>,
}

impl PlyExport {
    pub fn num_splats(&self) -> usize {
        self.num_splats
    }

    /// Size of the vertex data in bytes (excluding the header).
    pub fn body_len(&self) -> usize {
        self.num_splats * (11 + 3 * self.coeffs_per_channel) * size_of::<f32>()
    }

    /// Serialize the PLY (header + binary little-endian rows) into `writer`.
    pub fn write_to(&self, writer: impl Write) -> Result<(), ExportError> {
        serde_ply::to_writer(
            &PlyBody {
                vertex: VertexRows(self),
            },
            SerializeOptions::binary_le().with_comments(self.comments.clone()),
            writer,
        )?;
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ExportError> {
        const HEADER_SLACK: usize = 16 * 1024;
        let mut buf = Vec::with_capacity(self.body_len() + HEADER_SLACK);
        self.write_to(&mut buf)?;
        Ok(buf)
    }
}

#[derive(Serialize)]
struct PlyBody<'a> {
    vertex: VertexRows<'a>,
}

struct VertexRows<'a>(&'a PlyExport);

impl Serialize for VertexRows<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let data = self.0;
        let mut seq = serializer.serialize_seq(Some(data.num_splats))?;
        for i in 0..data.num_splats {
            seq.serialize_element(&VertexRow { data, i })?;
        }
        seq.end()
    }
}

struct VertexRow<'a> {
    data: &'a PlyExport,
    i: usize,
}

impl Serialize for VertexRow<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let d = self.data;
        let i = self.i;
        let n_coeffs = d.coeffs_per_channel;
        let rest_per_channel = n_coeffs - 1;

        let t = &d.transforms[i * 10..i * 10 + 10];
        let sh = &d.sh_coeffs[i * n_coeffs * 3..(i + 1) * n_coeffs * 3];
        let (r0, r1, r2, r3) = (t[3], t[4], t[5], t[6]);
        let rn = (r0 * r0 + r1 * r1 + r2 * r2 + r3 * r3).sqrt().max(1e-12);

        let mut state = serializer.serialize_struct("DynamicPlyGaussian", 14 + rest_per_channel * 3)?;
        state.serialize_field("x", &t[0])?;
        state.serialize_field("y", &t[1])?;
        state.serialize_field("z", &t[2])?;
        state.serialize_field("scale_0", &t[7])?;
        state.serialize_field("scale_1", &t[8])?;
        state.serialize_field("scale_2", &t[9])?;
        state.serialize_field("opacity", &d.raw_opacities[i])?;
        state.serialize_field("rot_0", &(r0 / rn))?;
        state.serialize_field("rot_1", &(r1 / rn))?;
        state.serialize_field("rot_2", &(r2 / rn))?;
        state.serialize_field("rot_3", &(r3 / rn))?;
        state.serialize_field("f_dc_0", &sh[0])?;
        state.serialize_field("f_dc_1", &sh[n_coeffs])?;
        state.serialize_field("f_dc_2", &sh[n_coeffs * 2])?;

        // f_rest_* are all red rest coeffs, then green, then blue.
        let rest = (0..3).flat_map(|c| &sh[c * n_coeffs + 1..(c + 1) * n_coeffs]);
        for (name, val) in SH_NAMES.iter().zip(rest) {
            state.serialize_field(name, val)?;
        }
        state.end()
    }
}

/// Read the splats back from the GPU into a [`PlyExport`].
pub async fn prepare_ply_export(
    splats: Splats,
    up_axis: Option<Vec3>,
) -> Result<PlyExport, ExportError> {
    // Fold any 3D-filter floor into the stored scales/opacity so the ply holds
    // ordinary derived values — the floor is never written as a separate field.
    let splats = splats.bake_min_scale();
    let sh_degree = splats.sh_degree();
    let num_splats = splats.num_splats() as usize;
    let render_mode_str = if splats.render_mip { "mip" } else { "default" };

    let data = Transaction::default()
        .register(splats.transforms.val())
        .register(splats.raw_opacities.val())
        .register(splats.sh_coeffs.val().permute([0, 2, 1])) // Permute to inria format ([n, channel, coeffs]).
        .execute_async()
        .await
        .map_err(|_fetch| ExportError::FetchFailed)?;
    drop(splats);

    let vecs: Vec<Vec<f32>> = data
        .into_iter()
        .map(|x| {
            x.try_into_vec()
                .map_err(|_convert| ExportError::DataConversion)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let [transforms, raw_opacities, sh_coeffs]: [Vec<f32>; 3] = vecs
        .try_into()
        .map_err(|_convert| ExportError::DataConversion)?;

    let coeffs_per_channel = sh_coeffs_for_degree(sh_degree) as usize;
    if transforms.len() != num_splats * 10
        || raw_opacities.len() != num_splats
        || sh_coeffs.len() != num_splats * coeffs_per_channel * 3
    {
        return Err(ExportError::DataConversion);
    }

    let mut comments = vec!["Exported from Brush".to_owned()];
    if let Some(up) = up_axis {
        comments.push(format!("Vertical axis: {} {} {}", up.x, up.y, up.z));
    } else {
        comments.push("Vertical axis: y".to_owned());
    }
    comments.push(format!("SH degree: {sh_degree}"));
    comments.push(format!("SplatRenderMode: {render_mode_str}"));

    Ok(PlyExport {
        transforms,
        raw_opacities,
        sh_coeffs,
        coeffs_per_channel,
        num_splats,
        comments,
    })
}

/// Stream the splats as a PLY into `writer`.
pub async fn splat_to_ply_writer(
    splats: Splats,
    up_axis: Option<Vec3>,
    writer: impl Write,
) -> Result<(), ExportError> {
    prepare_ply_export(splats, up_axis).await?.write_to(writer)
}

pub async fn splat_to_ply(splats: Splats, up_axis: Option<Vec3>) -> Result<Vec<u8>, ExportError> {
    prepare_ply_export(splats, up_axis).await?.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::load_splat_from_ply;
    use crate::test_utils::create_test_splats;

    use brush_render::gaussian_splats::SplatRenderMode;
    use std::io::Cursor;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[cfg(target_family = "wasm")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    async fn assert_coeffs_match(orig: &Splats, imported: &Splats) {
        let orig_sh: Vec<f32> = orig
            .sh_coeffs
            .val()
            .into_data_async()
            .await
            .unwrap()
            .try_into_vec()
            .expect("Failed to convert SH coefficients to vector");
        let import_sh: Vec<f32> = imported
            .sh_coeffs
            .val()
            .into_data_async()
            .await
            .unwrap()
            .try_into_vec()
            .expect("Failed to convert SH coefficients to vector");

        assert_eq!(orig_sh.len(), import_sh.len());
        for (i, (&orig, &imported)) in orig_sh.iter().zip(import_sh.iter()).enumerate() {
            assert!(
                (orig - imported).abs() < 1e-6_f32,
                "SH coeffs mismatch at index {i}: orig={orig}, imported={imported}",
            );
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_sh_degree_exports() {
        let _device = brush_cube::test_helpers::test_device().await;
        for degree in 0..=3 {
            let splats = create_test_splats(degree);
            assert_eq!(splats.sh_degree(), degree);

            let export = prepare_ply_export(splats.clone(), None).await.unwrap();
            let expected_rest_coeffs = (sh_coeffs_for_degree(degree) - 1) * 3;
            assert_eq!(
                export.body_len(),
                (14 + expected_rest_coeffs as usize) * size_of::<f32>()
            );
            assert!(splat_to_ply(splats, None).await.is_ok());
        }
    }

    /// The pre-streaming exporter, kept verbatim as a reference so the
    /// streaming writer can be checked for byte-identical output.
    mod legacy {
        use super::*;

        struct DynamicPlyGaussian {
            x: f32,
            y: f32,
            z: f32,
            scale_0: f32,
            scale_1: f32,
            scale_2: f32,
            opacity: f32,
            rot_0: f32,
            rot_1: f32,
            rot_2: f32,
            rot_3: f32,
            f_dc_0: f32,
            f_dc_1: f32,
            f_dc_2: f32,
            rest_coeffs: Vec<f32>,
        }

        impl Serialize for DynamicPlyGaussian {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let field_count = 14 + self.rest_coeffs.len();
                let mut state = serializer.serialize_struct("DynamicPlyGaussian", field_count)?;
                state.serialize_field("x", &self.x)?;
                state.serialize_field("y", &self.y)?;
                state.serialize_field("z", &self.z)?;
                state.serialize_field("scale_0", &self.scale_0)?;
                state.serialize_field("scale_1", &self.scale_1)?;
                state.serialize_field("scale_2", &self.scale_2)?;
                state.serialize_field("opacity", &self.opacity)?;
                state.serialize_field("rot_0", &self.rot_0)?;
                state.serialize_field("rot_1", &self.rot_1)?;
                state.serialize_field("rot_2", &self.rot_2)?;
                state.serialize_field("rot_3", &self.rot_3)?;
                state.serialize_field("f_dc_0", &self.f_dc_0)?;
                state.serialize_field("f_dc_1", &self.f_dc_1)?;
                state.serialize_field("f_dc_2", &self.f_dc_2)?;
                for (name, val) in SH_NAMES.iter().zip(&self.rest_coeffs) {
                    state.serialize_field(name, val)?;
                }
                state.end()
            }
        }

        #[derive(Serialize)]
        struct DynamicPly {
            vertex: Vec<DynamicPlyGaussian>,
        }

        pub async fn splat_to_ply(splats: Splats, up_axis: Option<Vec3>) -> Vec<u8> {
            let splats = splats.bake_min_scale();
            let sh_degree = splats.sh_degree();
            let data = Transaction::default()
                .register(splats.transforms.val())
                .register(splats.raw_opacities.val())
                .register(splats.sh_coeffs.val().permute([0, 2, 1]))
                .execute_async()
                .await
                .unwrap();
            let [transforms, raw_opacities, sh_coeffs]: [Vec<f32>; 3] = data
                .into_iter()
                .map(|x| x.try_into_vec().unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();

            let n = sh_coeffs_for_degree(sh_degree) as usize;
            let rest = n - 1;
            let vertex = (0..splats.num_splats() as usize)
                .map(|i| {
                    let s = &sh_coeffs[i * n * 3..(i + 1) * n * 3];
                    let [r, g, b] = [&s[0..n], &s[n..n * 2], &s[n * 2..n * 3]];
                    let pick = |c: &[f32]| -> Vec<f32> {
                        if c.len() > 1 && rest > 0 {
                            c[1..=rest].to_vec()
                        } else {
                            vec![]
                        }
                    };
                    let rest_coeffs = [pick(r), pick(g), pick(b)].concat();
                    let t = i * 10;
                    let (r0, r1, r2, r3) = (
                        transforms[t + 3],
                        transforms[t + 4],
                        transforms[t + 5],
                        transforms[t + 6],
                    );
                    let rn = (r0 * r0 + r1 * r1 + r2 * r2 + r3 * r3).sqrt().max(1e-12);
                    DynamicPlyGaussian {
                        x: transforms[t],
                        y: transforms[t + 1],
                        z: transforms[t + 2],
                        scale_0: transforms[t + 7],
                        scale_1: transforms[t + 8],
                        scale_2: transforms[t + 9],
                        rot_0: r0 / rn,
                        rot_1: r1 / rn,
                        rot_2: r2 / rn,
                        rot_3: r3 / rn,
                        opacity: raw_opacities[i],
                        f_dc_0: r[0],
                        f_dc_1: g[0],
                        f_dc_2: b[0],
                        rest_coeffs,
                    }
                })
                .collect();

            let render_mode_str = if splats.render_mip { "mip" } else { "default" };
            let mut comments = vec!["Exported from Brush".to_owned()];
            if let Some(up) = up_axis {
                comments.push(format!("Vertical axis: {} {} {}", up.x, up.y, up.z));
            } else {
                comments.push("Vertical axis: y".to_owned());
            }
            comments.push(format!("SH degree: {sh_degree}"));
            comments.push(format!("SplatRenderMode: {render_mode_str}"));

            serde_ply::to_bytes(
                &DynamicPly { vertex },
                SerializeOptions::binary_le().with_comments(comments),
            )
            .unwrap()
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_streaming_matches_legacy_bytes() {
        let _device = brush_cube::test_helpers::test_device().await;
        let up = Some(Vec3::new(0.0, 0.0, 1.0));
        for degree in 0..=3 {
            let splats = create_test_splats_with_non_unit_quats(degree, 257);
            for up_axis in [None, up] {
                let expected = legacy::splat_to_ply(splats.clone(), up_axis).await;
                let actual = splat_to_ply(splats.clone(), up_axis).await.unwrap();
                assert_eq!(actual, expected, "degree {degree}, up {up_axis:?}");

                let mut streamed = Vec::new();
                splat_to_ply_writer(splats.clone(), up_axis, &mut streamed)
                    .await
                    .unwrap();
                assert_eq!(streamed, expected, "writer, degree {degree}");
            }
        }
    }

    fn create_test_splats_with_non_unit_quats(sh_degree: u32, n: usize) -> Splats {
        let device: burn::tensor::Device = burn::backend::wgpu::WgpuDevice::default().into();
        let coeffs = sh_coeffs_for_degree(sh_degree) as usize;
        let f = |i: usize, k: usize| ((i * 31 + k * 7) % 97) as f32 * 0.013 - 0.6;
        let means = (0..n * 3).map(|k| f(k, 1) * 10.0).collect();
        let rotations = (0..n * 4).map(|k| f(k, 2) * 3.0 + 0.1).collect();
        let log_scales = (0..n * 3).map(|k| f(k, 3) - 2.0).collect();
        let sh = (0..n * coeffs * 3).map(|k| f(k, 4)).collect();
        let opac = (0..n).map(|k| f(k, 5) * 4.0).collect();
        Splats::from_raw(
            means,
            rotations,
            log_scales,
            sh,
            opac,
            SplatRenderMode::Default,
            &device,
        )
        .with_sh_degree(sh_degree)
    }

    /// Large-export smoke test. Run manually:
    /// `BRUSH_EXPORT_SPLATS=2764885 cargo test -p brush-serde --release -- --ignored large_export --nocapture`
    #[tokio::test]
    #[ignore]
    async fn large_export() {
        let _device = brush_cube::test_helpers::test_device().await;
        let n: usize = std::env::var("BRUSH_EXPORT_SPLATS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2_764_885);
        let splats = create_test_splats_with_non_unit_quats(3, n);
        let path = std::env::temp_dir().join("brush_large_export.ply");
        let unix_ms = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        };
        splats.transforms.val().into_data_async().await.unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        println!("PHASE export_start {}", unix_ms());
        let start = web_time::Instant::now();
        if std::env::var("BRUSH_EXPORT_LEGACY").is_ok() {
            std::fs::write(&path, legacy::splat_to_ply(splats, None).await).unwrap();
        } else {
            let file = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
            splat_to_ply_writer(splats, None, file).await.unwrap();
        }
        println!("PHASE export_end {}", unix_ms());
        let len = std::fs::metadata(&path).unwrap().len();
        println!(
            "exported {n} splats -> {len} bytes in {:?} ({})",
            start.elapsed(),
            path.display()
        );

        let reader = tokio::io::BufReader::new(tokio::fs::File::open(&path).await.unwrap());
        let msg = load_splat_from_ply(reader, None).await.unwrap();
        assert_eq!(msg.data.num_splats(), n);
        let _ = std::fs::remove_file(&path);
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_ply_field_count_matches_sh_degree() {
        let _device = brush_cube::test_helpers::test_device().await;
        let test_cases = [(0, 0), (1, 9), (2, 24)];

        for (degree, expected_rest_fields) in test_cases {
            let splats = create_test_splats(degree);
            let ply_bytes = splat_to_ply(splats, None).await.unwrap();
            let ply_string = String::from_utf8_lossy(&ply_bytes);

            let actual_rest_fields = ply_string.matches("property float f_rest_").count();
            assert_eq!(
                actual_rest_fields, expected_rest_fields,
                "Degree {degree} should have {expected_rest_fields} f_rest_ fields",
            );

            assert!(ply_string.contains("f_dc_0"));
            if expected_rest_fields > 0 {
                assert!(ply_string.contains("f_rest_0"));
                assert!(!ply_string.contains(&format!("f_rest_{expected_rest_fields}")));
            } else {
                assert!(!ply_string.contains("f_rest_0"));
            }
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_roundtrip_sh_coefficient_ordering() {
        let device: burn::tensor::Device = brush_cube::test_helpers::test_device().await.into();

        for degree in [0, 1, 2] {
            let original_splats = create_test_splats(degree);
            let ply_bytes = splat_to_ply(original_splats.clone(), None)
                .await
                .expect("Failed to serialize splats");

            let cursor = Cursor::new(ply_bytes);
            let imported_message = load_splat_from_ply(cursor, None)
                .await
                .expect("Failed to deserialize splats");
            let imported_splats = imported_message
                .data
                .into_splats(&device, SplatRenderMode::Default);

            assert_eq!(imported_splats.sh_degree(), degree);
            assert_coeffs_match(&original_splats, &imported_splats).await;
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_export_roundtrip_multiple_splats() {
        use crate::test_utils::create_test_splats_with_count;

        let device: burn::tensor::Device = brush_cube::test_helpers::test_device().await.into();
        let num_splats = 100;

        for degree in [0, 1, 2, 3] {
            let original = create_test_splats_with_count(degree, num_splats);
            assert_eq!(original.num_splats(), num_splats as u32);

            let ply_bytes = splat_to_ply(original.clone(), None)
                .await
                .expect("Failed to export splats");

            assert!(!ply_bytes.is_empty(), "Exported PLY should not be empty");

            let cursor = Cursor::new(ply_bytes);
            let imported_message = load_splat_from_ply(cursor, None)
                .await
                .expect("Failed to reimport exported splats");
            let imported = imported_message
                .data
                .into_splats(&device, SplatRenderMode::Default);

            assert_eq!(
                imported.num_splats(),
                num_splats as u32,
                "Splat count mismatch after roundtrip"
            );
            assert_eq!(imported.sh_degree(), degree);
            assert_coeffs_match(&original, &imported).await;
        }
    }
}
