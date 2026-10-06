use crate::session;
use eyre::{Context, ContextCompat, Result};
use ndarray::{ArrayBase, Axis, IxDyn, ViewRepr};
use std::{cmp::Ordering, collections::VecDeque, path::Path};

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub samples: Vec<i16>,
}

fn find_max_index(row: ArrayBase<ViewRepr<&f32>, IxDyn>) -> Result<usize> {
    let (max_index, _) = row
        .iter()
        .enumerate()
        .max_by(|a, b| {
            a.1.partial_cmp(b.1)
                .context("Comparison error")
                .unwrap_or(Ordering::Equal)
        })
        .context("sub_row should not be empty")?;
    Ok(max_index)
}

pub fn get_segments<P: AsRef<Path>>(
    samples: &[i16],
    sample_rate: u32,
    model_path: P,
) -> Result<impl Iterator<Item = Result<Segment>> + '_> {
    // Create session using the provided model path
    let mut session = session::create_session(model_path.as_ref())?;

    // Define frame parameters
    let frame_size = 270;
    let frame_start = 721;
    let window_size = (sample_rate * 10) as usize; // 10 seconds
    let mut is_speeching = false;
    let mut offset = frame_start;
    let mut start_offset = 0.0;

    // Pad end with silence for full last segment
    let padded_samples = {
        let mut padded = Vec::from(samples);
        padded.extend(vec![0; window_size - (samples.len() % window_size)]);
        padded
    };

    // Iterator of window start offsets. We MUST NOT advance this on every
    // closure invocation — only when the current window's buffered segments
    // have been fully drained. Otherwise a window that produces zero segments
    // (e.g. a silent opening 10s) terminates the whole iterator via
    // segments_queue.pop_front() -> None, and every later window is skipped.
    let mut next_window_start = 0usize;
    let n_windows = padded_samples.len().div_ceil(window_size);

    let mut segments_queue: VecDeque<Segment> = VecDeque::new();
    let mut session = Some(session);
    Ok(std::iter::from_fn(move || {
        loop {
            // 1. Drain any buffered segments from the current window first.
            if let Some(seg) = segments_queue.pop_front() {
                return Some(Ok(seg));
            }
            // 2. No buffered segments: advance to the next window, or end.
            if next_window_start >= n_windows {
                return None;
            }
            let win_idx = next_window_start;
            next_window_start += 1;

            let start = win_idx * window_size;
            let end = (start + window_size).min(padded_samples.len());
            let window = &padded_samples[start..end];


            // Convert window to ndarray::Array1
            let array = ndarray::Array1::from_iter(window.iter().map(|&x| x as f32));
            let array = array.view().insert_axis(Axis(0)).insert_axis(Axis(1));

            let inputs = ort::inputs![ort::value::TensorRef::from_array_view(array.into_dyn())
                .map_err(|e| eyre::eyre!("Failed to prepare inputs: {:?}", e))
                .ok()?];

            let sess = session.as_mut().expect("session alive");
            let ort_outs = match sess.run(inputs) {
                Ok(outputs) => outputs,
                Err(e) => return Some(Err(eyre::eyre!("Failed to run the session: {:?}", e))),
            };

            let ort_out = match ort_outs.get("output").context("Output tensor not found") {
                Ok(output) => output,
                Err(e) => return Some(Err(eyre::eyre!("Output tensor error: {:?}", e))),
            };

            let ort_out = match ort_out
                .try_extract_tensor::<f32>()
                .context("Failed to extract tensor")
            {
                Ok(tensor) => tensor,
                Err(e) => return Some(Err(eyre::eyre!("Tensor extraction error: {:?}", e))),
            };

            let (shape, data) = ort_out;
            let shape_slice: Vec<usize> = (0..shape.len()).map(|i| shape[i] as usize).collect();
            let view =
                ndarray::ArrayViewD::<f32>::from_shape(ndarray::IxDyn(&shape_slice), data).unwrap();

            // Process the whole window. Zero-segment (silent) windows still fall
            // through to the `loop` and advance to the next window.
            for row in view.outer_iter() {
                for sub_row in row.axis_iter(Axis(0)) {
                    let max_index = match find_max_index(sub_row) {
                        Ok(index) => index,
                        Err(e) => return Some(Err(e)),
                    };

                    if max_index != 0 {
                        if !is_speeching {
                            start_offset = offset as f64;
                            is_speeching = true;
                        }
                    } else if is_speeching {
                        let start = start_offset / sample_rate as f64;
                        let end = offset as f64 / sample_rate as f64;

                        let start_f64 = start * (sample_rate as f64);
                        let end_f64 = end * (sample_rate as f64);

                        // Ensure indices are within bounds
                        let start_idx = start_f64.min((samples.len() - 1) as f64) as usize;
                        let end_idx = end_f64.min(samples.len() as f64) as usize;

                        let segment_samples = &padded_samples[start_idx..end_idx];

                        is_speeching = false;

                        let segment = Segment {
                            start,
                            end,
                            samples: segment_samples.to_vec(),
                        };
                        segments_queue.push_back(segment);
                    }
                    offset += frame_size;
                }
            }
            // If this window produced segments, the top of the loop returns them
            // on the next iterations. If not, the loop advances to the next window.
        }
    }))
}
