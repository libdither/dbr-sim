#![allow(dead_code)]

mod router;
use router::InternetRouter;

use std::{collections::HashMap, fmt::Debug};
use std::any::Any;

use plotters::prelude::*;
use plotters::coord::types::RangedCoordf32;
use nalgebra::Vector2;
use rand::Rng;

use crate::node::{NodeID, RouteCoord};

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
			router: Default::default(),
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
			for node in self.nodes.values_mut() {
				// Get Packets going to node
				let incoming_packets = self.router.tick_node(node.net_id());
				// Get packets coming from node
				let mut outgoing_packets = node.tick(incoming_packets);

				// Make outgoing packets have the correct return address or parse request
				for packet in &mut outgoing_packets {
					packet.src_addr = node.net_id();
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
			}
		}
	}
	pub fn gen_routing_plot(&self, path: &str, dimensions: (u32, u32)) -> Result<(), Box<dyn std::error::Error>> {
		let root = BitMapBackend::new(path, dimensions).into_drawing_area();

		root.fill(&RGBColor(200,200,200))?;

		// Make sure it uses correct graph layout with 4 quadrants
		let root = root.apply_coord_spec(Cartesian2d::<RangedCoordf32, RangedCoordf32>::new(
			-1f32..1f32,
			1f32..-1f32,
			(0..dimensions.0 as i32, 0..dimensions.1 as i32),
		));
		use plotters::style::text_anchor::{Pos, HPos, VPos};
		let dot_and_label = |x: f32, y: f32, label: &str| {
			return EmptyElement::at((x, y))
				+ Circle::new((0, 0), 10, ShapeStyle::from(&BLACK).filled())
				+ Text::new(
					format!("{}", label),
					(0, 0),
					("sans-serif", 15.0).into_font().color(&WHITE).pos(Pos::new(HPos::Center, VPos::Center)),
				);
		};

		let convert_coords = |position: (i32, i32)| {
			Vector2::new(position.0 as f32 / (router::AREA / 2) as f32, position.1 as f32 / (router::AREA / 2) as f32)
		};

		for (net_id, node) in &self.nodes {
			let node = node.as_any().downcast_ref::<crate::node::Node>().unwrap();
			if node.node_list.len() > 0 {
				let node_coord = convert_coords(self.router.speed_map.get(net_id).ok_or("failed to index speed map")?.position);
				for (index, (_, node_id)) in node.node_list.iter().enumerate() {
					let remote_session = node.remote(node_id)?.session()?;
					let remote_net_id = remote_session.return_net_id;
					let remote_coord = convert_coords(self.router.speed_map[&remote_net_id].position);
					let color = if index < 3 { &BLACK } else { &RGBColor(255, 255, 255) };
					
					// offset connections so both directions show side by side
					let offset = (nalgebra::Rotation2::new(std::f32::consts::FRAC_PI_2) * (node_coord - remote_coord)).normalize();
					let offset_node_coord = node_coord + (offset * 0.01);
					let offset_remote_coord = remote_coord + (offset * 0.01);
					root.draw(&PathElement::new([(offset_node_coord.x, offset_node_coord.y), (offset_remote_coord.x, offset_remote_coord.y)], ShapeStyle::from(color)))?;
				}
			}
		}

		for (net_id, lc) in self.router.speed_map.iter() {
			let vec = convert_coords(lc.position);
			root.draw(&dot_and_label(vec.x, vec.y, &net_id.to_string()))?;
		}
		Ok(())
	}
}
