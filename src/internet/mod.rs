//#![allow(dead_code)]

use std::{collections::HashMap, fmt::Debug};
use std::any::Any;
use std::ops::Range;

use rand::Rng;
use petgraph::Graph;
use nalgebra::Point2;
use plotters::style::RGBColor;

mod router;
use router::InternetRouter;

use crate::node::{Node, NodeID, RouteCoord};

pub const FIELD_DIMENSIONS: (Range<i32>, Range<i32>) = (0..640, 0..360);

pub type InternetID = usize;

#[derive(Debug)]
pub enum InternetRequest {
	RouteCoordDHTRead(NodeID),
	RouteCoordDHTWrite(NodeID, RouteCoord),
	RouteCoordDHTResponse(Option<(NodeID, RouteCoord)>),
}

#[derive(Default, Debug)]
pub struct InternetPacket {
	pub dest_addr: InternetID,
	pub data: Vec<u8>,
	pub src_addr: InternetID,
	pub request: Option<InternetRequest>,
}
impl InternetPacket { pub fn gen_request(dest_addr: InternetID, request: InternetRequest) -> Self { Self { dest_addr, data: vec![], src_addr: dest_addr, request: Some(request) } } }

pub trait CustomNode: std::fmt::Debug {
	type CustomNodeAction;
	fn net_id(&self) -> InternetID;
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket>;
	fn action(&mut self, action: Self::CustomNodeAction);
	fn as_any(&self) -> &dyn Any;
	fn set_deus_ex_data(&mut self, data: Option<RouteCoord>);
}

#[derive(Debug)]
pub struct InternetSim<CN: CustomNode> {
	pub nodes: HashMap<InternetID, CN>,
	pub router: InternetRouter,
	route_coord_dht: HashMap<NodeID, RouteCoord>,
}
impl<CN: CustomNode> InternetSim<CN> {
	pub fn new() -> InternetSim<CN> {
		InternetSim {
			nodes: HashMap::new(),
			router: InternetRouter::new(FIELD_DIMENSIONS),
			route_coord_dht: HashMap::new(),
		}
	}
	pub fn lease(&self) -> InternetID { self.nodes.len() }
	pub fn add_node(&mut self, node: CN) { self.nodes.insert(node.net_id(), node); } 
	pub fn del_node(&mut self, net_id: InternetID) { self.nodes.remove(&net_id); }
	pub fn node_mut(&mut self, node_id: InternetID) -> Option<&mut CN> { self.nodes.get_mut(&node_id) }
	pub fn node(&self, node_id: InternetID) -> Option<&CN> { self.nodes.get(&node_id) }
	pub fn tick(&mut self, ticks: usize, rng: &mut impl Rng) {
		//let packets_tmp = Vec::new();
		for _ in 0..ticks {
			for (&node_net_id, node) in self.nodes.iter_mut() {
				// Get Packets going to node
				let incoming_packets = self.router.tick_node(node_net_id);
				// Get packets coming from node
				let mut outgoing_packets = node.tick(incoming_packets);

				// Make outgoing packets have the correct return address or parse request
				for packet in &mut outgoing_packets {
					packet.src_addr = node_net_id;
					match packet.request {
						Some(InternetRequest::RouteCoordDHTRead(node_id)) => {
							packet.dest_addr = packet.src_addr;
							packet.request = Some(InternetRequest::RouteCoordDHTResponse(self.route_coord_dht.get(&node_id).map(|&rc|(node_id,rc))))
						},
						Some(InternetRequest::RouteCoordDHTWrite(node_id, route_coord)) => {
							packet.dest_addr = packet.src_addr;
							let old_route = self.route_coord_dht.insert(node_id, route_coord);
							packet.request = Some(InternetRequest::RouteCoordDHTResponse( old_route.map(|r|(node_id, r) )));
						}
						_ => {},
					} 
				}
				// Send packets through the router
				self.router.add_packets(outgoing_packets, rng);
				if let Some(rn) = self.router.node_map.get(&node_net_id) {
					let cheat_coord = rn.position.clone().map(|s|s.floor() as i64);
					node.set_deus_ex_data( Some(cheat_coord) ) }
			}
		}
	}
}

use crate::plot::GraphPlottable;
impl GraphPlottable for InternetSim<Node> {
	fn gen_graph(&self) -> Graph<(String, Point2<i32>), RGBColor> {
		//let root = BitMapBackend::new(path, dimensions).into_drawing_area();
		/* for (idx, node) in &self.nodes {

		} */
		/* use petgraph::data::FromElements;
		let nodes: Vec<String, Point2<i32>> = self.router.node_map.iter().map(|(&net_id, lc)|{
			(
				net_id.to_string(),
				lc.position.cast(),
			)
		}).collect();
		println!("nodes: {:?}", nodes); */

		//let edge_array = self.nodes

		//let node_graph: Graph<(String, nalgebra::Point2<f32>), RGBColor> = .from_elements();
		//let plot_edge_iter = self.nodes.iter().filter_map(|(&id, n)|node.as_any().downcast_ref::<crate::node::Node>());
		//graph
		Graph::with_capacity(0, 0)
	}
}
