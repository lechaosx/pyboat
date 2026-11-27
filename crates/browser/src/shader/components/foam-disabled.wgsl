// Passes that never draw water read no foam.
fn foam_cover(offset_m: vec2<f32>, thinning: f32) -> FoamCover {
    return no_foam_cover();
}
