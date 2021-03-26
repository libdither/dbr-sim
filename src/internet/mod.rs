//#![allow(dead_code)]

use std::{collections::HashMap, fmt::Debug};
use std::any::Any;
use std::ops::Range;

use rand::Rng;
use petgraph::Graph;
use nalgebra::Point2;
use plotters::style::RGBColor;
use smallvec::SmallVec;

mod router;
use router::InternetRouter;

use crate::node::{Node, NodeID, RouteCoord};

pub const FIELD_DIMENSIONS: (Range<i32>, Range<i32>) = (-320..320, -130..130);

pub type NetAddr = u128;
pub type PacketVec = SmallVec<[InternetPacket; 32]>;

#[derive(Debug)]
pub enum InternetRequest {
	RouteCoordDHTRead(NodeID),
	RouteCoordDHTWrite(NodeID, RouteCoord),
	RouteCoordDHTReadResponse(NodeID, Option<RouteCoord>),
	RouteCoordDHTWriteResponse(Option<(NodeID, RouteCoord)>),
}

#[derive(Default, Debug)]
pub struct InternetPacket {
	pub dest_addr: NetAddr,
	pub data: Vec<u8>,
	pub src_addr: NetAddr,
	pub request: Option<InternetRequest>,
}
impl InternetPacket { pub fn gen_request(dest_addr: NetAddr, request: InternetRequest) -> Self { Self { dest_addr, data: vec![], src_addr: dest_addr, request: Some(request) } } }

pub trait CustomNode: std::fmt::Debug {
	type CustomNodeAction;
	fn net_addr(&self) -> NetAddr;
	fn tick(&mut self, incoming: PacketVec) -> PacketVec;
	fn action(&mut self, action: Self::CustomNodeAction);
	fn as_any(&self) -> &dyn Any;
	fn set_deus_ex_data(&mut self, data: Option<RouteCoord>);
}

#[derive(Debug)]
pub struct InternetSim<CN: CustomNode> {
	pub nodes: HashMap<NetAddr, CN>,
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
	pub fn lease(&self) -> NetAddr { self.nodes.len() as NetAddr }
	pub fn add_node(&mut self, node: CN, rng: &mut impl Rng) {
		self.router.add_node(node.net_addr(), rng);
		self.nodes.insert(node.net_addr(), node);
	}
	pub fn del_node(&mut self, net_addr: NetAddr) { self.nodes.remove(&net_addr); }
	pub fn node_mut(&mut self, net_addr: NetAddr) -> Option<&mut CN> { self.nodes.get_mut(&net_addr) }
	pub fn node(&self, net_addr: NetAddr) -> Option<&CN> { self.nodes.get(&net_addr) }
	pub fn tick(&mut self, ticks: usize, rng: &mut impl Rng) {
		//let packets_tmp = Vec::new();
		for _ in 0..ticks {
			for (&node_net_addr, node) in self.nodes.iter_mut() {
				// Get Packets going to node
				let incoming_packets = self.router.tick_node(node_net_addr);
				// Get packets coming from node
				let mut outgoing_packets = node.tick(incoming_packets);

				// Make outgoing packets have the correct return address or parse request
				for packet in &mut outgoing_packets {
					packet.src_addr = node_net_addr;
					if let Some(request) = &packet.request {
						log::debug!("NetAddr({:?}) Requested InternetRequest::{:?}", node_net_addr, request);
						packet.request = Some(match *request {
							InternetRequest::RouteCoordDHTRead(node_id) => {
								packet.dest_addr = packet.src_addr;
								let route = self.route_coord_dht.get(&node_id).map(|r|r.clone());
								InternetRequest::RouteCoordDHTReadResponse(node_id, route)
							},
							InternetRequest::RouteCoordDHTWrite(node_id, route_coord) => {
								packet.dest_addr = packet.src_addr;
								let old_route = self.route_coord_dht.insert(node_id, route_coord);
								InternetRequest::RouteCoordDHTWriteResponse( old_route.map(|r|(node_id, r) ))
							}
							_ => { log::error!("Invalid InternetRequest variant"); unimplemented!() },
						});
					}
				}
				// Send packets through the router
				self.router.add_packets(outgoing_packets, rng);
				if let Some(rn) = self.router.node_map.get(&node_net_addr) {
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
		let nodes: Vec<Element<(String, Point2<i32>),RGBColor>> = self.router.node_map.iter().map(|(&net_addr, lc)|{
			Element::Node {
				weight: (
					net_addr.to_string(),
					lc.position.map(|i|i as i32),
				)
			}
		}).collect();

		let node_idx_map = &self.router.node_map.iter().enumerate().map(|(idx,(&id,_))|(id,idx)).collect::<HashMap<NetAddr,usize>>();

		let edges = self.nodes.iter().enumerate().map(|(_, (net_addr, node))|{
			node.remotes.iter().filter_map(move |(_,remote)|{
				// Set color based on 
				remote.session().ok().map(|s|{
					s.direct().ok().map(|d|{
						let color = if s.is_peer() { RGBColor(0,0,0) } else { RGBColor(255,255,255) };
						(d.net_addr, color)
					})
				}).flatten()
			}).map(move |(remote_net_addr, color)|{
				Element::Edge {
					source: node_idx_map[&net_addr],
					target: node_idx_map[&remote_net_addr],
					weight: color,
				}
			})
		}).flatten();
		let graph = Graph::from_elements(nodes.into_iter().chain(edges));
		graph
	}
}
