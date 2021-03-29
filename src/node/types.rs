#![allow(dead_code)]

pub use crate::node::session::{RemoteSession, SessionError, SessionType, RoutedSession};

use vpsearch::MetricSpace;
use nalgebra::Point2;

/// Hash uniquely identifying a node (represents the Multihash of the node's Public Key)
pub type NodeID = u32;
/// Number uniquely identifying a session, represents a Symmetric key
pub type SessionID = u32;
/// Coordinate that represents a position of a node relative to other nodes in 2D space.
pub type RouteScalar = u64;

//#[repr(transparent)]
pub type RouteCoord = Point2<i64>;

pub struct RouteCoordStruct {
	x: i64,
	y: i64,
}
impl From<Point2<i64>> for RouteCoordStruct {
	fn from(other: Point2<i64>) -> RouteCoordStruct {
		RouteCoordStruct { x: other[0], y: other[1] }
	}
}
pub fn route_dist(start: &RouteCoord, end: &RouteCoord) -> f64 {
	let start_f64 = start.map(|s|s as f64);
	let end_f64 = end.map(|s|s as f64);
	nalgebra::distance(&start_f64, &end_f64)
}

struct MyImpl;
use crate::node::NodeIdx;
impl MetricSpace<MyImpl> for RouteCoord {
    type UserData = NodeIdx;
    type Distance = f64;

    fn distance(&self, other: &Self, _: &Self::UserData) -> Self::Distance {
        let dx = self.x - other.x;
		let dy = self.y - other.y;
        f64::sqrt((dx*dx + dy*dy) as f64) // sqrt is required
    }
}