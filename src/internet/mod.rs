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

pub const FIELD_DIMENSIONS: (Range<i32>, Range<i32>) = (-320..320, -130..130);

pub type InternetID = u128;

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
	pub fn lease(&self) -> InternetID { self.nodes.len() as InternetID }
	pub fn add_node(&mut self, node: CN) { self.nodes.insert(node.net_id(), node); } 
	pub fn del_node(&mut self, net_id: InternetID) { self.nodes.remove(&net_id); }
	pub fn node_mut(&mut self, net_id: InternetID) -> Option<&mut CN> { self.nodes.get_mut(&net_id) }
	pub fn node(&self, net_id: InternetID) -> Option<&CN> { self.nodes.get(&net_id) }
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
		use petgraph::data::{FromElements, Element};
		let nodes: Vec<Element<(String, Point2<i32>),RGBColor>> = self.router.node_map.iter().map(|(&net_id, lc)|{
			Element::Node {
				weight: (
					net_id.to_string(),
					lc.position.map(|i|i as i32),
				)
			}
		}).collect();

		let node_idx_map = &self.nodes.iter().enumerate().map(|(idx,(&id,_))|(id,idx)).collect::<HashMap<InternetID,usize>>();

		let edges = self.nodes.iter().enumerate().map(|(_, (net_id, node))|{
			let src_index = node_idx_map[net_id];
			node.node_list.iter().filter_map(move |(_,&remote_id)|{
				// Get Net ID and set color based on peerage
				node.remotes[&remote_id].session().map(|s|{
					(s.return_net_id, if s.is_peer() {RGBColor(0,0,0)} else {RGBColor(255,255,255)})
				}).ok()

			}).map(move |(remote_net_id, color)|{
				Element::Edge {
					source: src_index.clone(),
					target: node_idx_map[&remote_net_id],
					weight: color,
				}
			})
		}).flatten();
		let graph = Graph::from_elements(nodes.into_iter().chain(edges));
		graph
	}
}
