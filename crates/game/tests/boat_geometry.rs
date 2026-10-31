use game::dinghy_parameters;
use rapier2d::prelude::Vector;

#[test]
fn transom_hung_rudder_pivots_on_the_hull_centerline_at_the_stern() {
    let parameters = dinghy_parameters();
    assert_eq!(
        parameters.rudder_position_m,
        Vector::new(-parameters.hull.length_m / 2.0, 0.0)
    );
}
