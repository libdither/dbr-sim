#![allow(dead_code)]

mod router;
use router::InternetRouter;

use std::collections::HashMap;
use std::any::Any;

use plotters::prelude::*;
use plotters::coord::types::RangedCoordf32;

pub type InternetID = usize;

#[derive(Default, Debug)]
pub struct InternetPacket {
	pub dest_addr: InternetID,
	pub data: Vec<u8>,
	pub src_addr: InternetID,
}

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
}
impl<CN: CustomNode> InternetSim<CN> {
	pub fn new() -> InternetSim<CN> {
		InternetSim {
			nodes: HashMap::new(),
			router: Default::default(),
		}
	}
	pub fn lease(&self) -> InternetID { self.nodes.len() }
	pub fn add_node(&mut self, node: CN) { self.nodes.insert(node.net_id(), node); }
	pub fn del_node(&mut self, net_id: InternetID) { self.nodes.remove(&net_id); }
	pub fn node_mut(&mut self, node_id: InternetID) -> Option<&mut CN> { self.nodes.get_mut(&node_id) }
	pub fn node(&self, node_id: InternetID) -> Option<&CN> { self.nodes.get(&node_id) }
	pub fn run(&mut self, ticks: usize) {
		//let packets_tmp = Vec::new();
		for _ in 0..ticks {
			for node in self.nodes.values_mut() {
				// Get Packets going to node
				let incoming_packets = self.router.tick_node(node.net_id());
				// Get packets coming from node
				let mut outgoing_packets = node.tick(incoming_packets);
				// Make outgoing packets have the correct return address
				for packet in &mut outgoing_packets { packet.src_addr = node.net_id(); }
				// Send packets through the router
				self.router.add_packets(outgoing_packets);
			}
		}
	}
	pub fn gen_routing_plot(&self, path: &str, dimensions: (u32, u32)) -> Result<(), Box<dyn std::error::Error>> {
		let root = BitMapBackend::new(path, dimensions).into_drawing_area();

		root.fill(&RGBColor(240, 200, 200))?;
		
		let root = root.apply_coord_spec(Cartesian2d::<RangedCoordf32, RangedCoordf32>::new(
			-1f32..1f32,
			-1f32..1f32,
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
			(position.0 as f32 / (router::AREA / 2) as f32, position.1 as f32 / (router::AREA / 2) as f32)
		};

		for (net_id, node) in &self.nodes {
			let node = node.as_any().downcast_ref::<crate::node::Node>().unwrap();
			let node_coord = convert_coords(self.router.speed_map[net_id].position);
			for (session_id, _) in &node.peered_nodes {
				let remote_net_id = node.remote(&node.sessions[session_id])?.session()?.return_net_id;
				let remote_coord = convert_coords(self.router.speed_map[&remote_net_id].position);
				root.draw(&PathElement::new([node_coord, remote_coord], ShapeStyle::from(&BLACK)))?;
			}
		}

		for (net_id, lc) in self.router.speed_map.iter() {
			let (x, y) = convert_coords(lc.position);
			root.draw(&dot_and_label(x, y, &net_id.to_string()))?;
		}
		Ok(())
	}
}
