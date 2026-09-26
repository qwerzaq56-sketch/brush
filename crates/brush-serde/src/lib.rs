#![recursion_limit = "256"]

pub mod export;
pub mod import;
pub mod ply_gaussian;
pub mod quant;

// Re-export main functionality
pub use export::{EXPORT_CHUNK_SPLATS, ExportError, splat_to_ply, splat_to_ply_writer};
pub use import::{
    ParseMetadata, SplatData, SplatMessage, load_splat_from_ply, stream_splat_from_ply,
};
pub use ply_gaussian::PlyGaussian;

// Re-export serde-ply types for compatibility
pub use serde_ply::DeserializeError;

/// Test-only allocator that tracks live and peak Rust heap bytes, so export
/// memory can be measured without GPU driver allocations mixed in.
#[cfg(all(test, not(target_family = "wasm")))]
pub(crate) mod heap_track {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CURRENT: AtomicUsize = AtomicUsize::new(0);
    static PEAK: AtomicUsize = AtomicUsize::new(0);

    struct Tracking;

    // SAFETY: forwards every call to the system allocator unchanged.
    unsafe impl GlobalAlloc for Tracking {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: same contract as the caller's.
            let p = unsafe { System.alloc(layout) };
            if !p.is_null() {
                let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                PEAK.fetch_max(now, Ordering::Relaxed);
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: same contract as the caller's.
            unsafe { System.dealloc(ptr, layout) };
            CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: same contract as the caller's.
            let p = unsafe { System.realloc(ptr, layout, new_size) };
            if !p.is_null() {
                if new_size >= layout.size() {
                    let now = CURRENT.fetch_add(new_size - layout.size(), Ordering::Relaxed)
                        + (new_size - layout.size());
                    PEAK.fetch_max(now, Ordering::Relaxed);
                } else {
                    CURRENT.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
                }
            }
            p
        }
    }

    #[global_allocator]
    static GLOBAL: Tracking = Tracking;

    /// Current live heap bytes; also resets the peak to this value.
    pub fn reset_peak() -> usize {
        let now = CURRENT.load(Ordering::Relaxed);
        PEAK.store(now, Ordering::Relaxed);
        now
    }

    pub fn peak() -> usize {
        PEAK.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[allow(unused)]
mod test_utils {
    use brush_render::gaussian_splats::{SplatRenderMode, Splats};
    use brush_render::sh::sh_coeffs_for_degree;
    use burn::backend::wgpu::WgpuDevice;
    use burn::tensor::Device;

    pub fn create_test_splats(sh_degree: u32) -> Splats {
        create_test_splats_with_count(sh_degree, 1)
    }

    pub fn create_test_splats_with_count(sh_degree: u32, num_splats: usize) -> Splats {
        let device: Device = WgpuDevice::default().into();
        let coeffs_per_channel = sh_coeffs_for_degree(sh_degree) as usize;

        let mut means = Vec::new();
        let mut rotations = Vec::new();
        let mut log_scales = Vec::new();
        let mut sh_coeffs = Vec::new();
        let mut opacities = Vec::new();

        for i in 0..num_splats {
            let offset = i as f32;

            means.extend([offset, offset + 1.0, offset + 2.0]);
            rotations.extend([1.0, 0.0, 0.0, 0.0]);
            log_scales.extend([
                -0.1 + offset * 0.05,
                0.2 + offset * 0.05,
                -0.3 + offset * 0.05,
            ]);

            for _ in 0..3 {
                sh_coeffs.push(0.5 + offset * 0.1);
                for j in 1..coeffs_per_channel {
                    sh_coeffs.push(j as f32 * 0.1 + offset * 0.01);
                }
            }

            opacities.push(0.8 - offset * 0.1);
        }

        Splats::from_raw(
            means,
            rotations,
            log_scales,
            sh_coeffs,
            opacities,
            SplatRenderMode::Default,
            &device,
        )
        .with_sh_degree(sh_degree)
    }
}
