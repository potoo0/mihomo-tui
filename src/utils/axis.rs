use crate::utils::byte_size::human_bytes;

pub fn axis_bounds(data: &[(f64, f64)]) -> (f64, f64) {
    let min_y = data.iter().map(|(_, v)| *v).reduce(f64::min).unwrap_or(0.0);
    let max_y = data.iter().map(|(_, v)| *v).reduce(f64::max).unwrap_or(0.0) + 1.0;
    (min_y, max_y)
}

pub fn axis_labels(lower: f64, high: f64) -> Vec<String> {
    let labels = if (high - lower) <= 1.0 + f64::EPSILON {
        vec![lower, high]
    } else {
        vec![lower, (lower + high) / 2.0, high]
    };

    labels.iter().map(|v| human_bytes(*v, None)).collect()
}
