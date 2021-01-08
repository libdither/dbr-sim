#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;

pub type InternetID = usize;

#[derive(Default, Debug)]
pub struct InternetPacket {
	pub dest_addr: InternetID,
	pub data: Vec<u8>,
	pub src_addr: InternetID,
}

pub trait CustomNode: Default {
	fn net_id(&self) -> InternetID;
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket>;
}

#[derive(Default)]
pub struct InternetRouter {
	speed_map: HashMap<(InternetID, InternetID), usize>,
	packet_map: HashMap<InternetID, Vec<(InternetPacket, usize)>>,
}
impl InternetRouter {
	fn add_packets(&mut self, packets: Vec<InternetPacket>) {
		for packet in packets {
			let index = (packet.src_addr, packet.dest_addr);

			let latency = if let Some(speed) = self.speed_map.get(&index) {
				*speed
			} else {
				let random_speed = rand::random::<usize>() % 10 + 1;
				self.speed_map.insert(index, random_speed);
				random_speed
			};
			if let Some(packet_stream) = self.packet_map.get_mut(&packet.dest_addr) {
				packet_stream.push((packet, latency));
			} else {
				self.packet_map
					.insert(packet.dest_addr, vec![(packet, latency)]);
			}
		}
	}
	fn tick_node(&mut self, destination: InternetID) -> Vec<InternetPacket> {
		if let Some(packets) = self.packet_map.get_mut(&destination) {
			packets.iter_mut().for_each(|item| item.1 -= 1); // Decrement ticks
			//let tmp_packets = packets.iter().filter(|x| x.1 < 0).map(|x| x.0)
			
			packets.drain_filter(|x| x.1 <= 0).map(|x| x.0).collect()
		} else {
			return Vec::new();
		}
	}
}

#[derive(Default)]
pub struct InternetSim<CN: CustomNode> {
	nodes: HashMap<InternetID, CN>,
	router: InternetRouter,
}
impl<CN: CustomNode> InternetSim<CN> {
	pub fn new() -> InternetSim<CN> {
		InternetSim {
			nodes: HashMap::new(),
			..Default::default()
		}
	}
	pub fn add(&mut self, node: CN) -> InternetID {
		let id: InternetID = self.nodes.len() + 1;
		self.nodes.insert(id, node);
		id
	}
	pub fn delete(&mut self, node_id: InternetID) {
		self.nodes.remove(&node_id);
	}
	pub fn node_mut(&mut self, node_id: InternetID) -> Option<&mut CN> {
		self.nodes.get_mut(&node_id)
	}
	pub fn run(&mut self, ticks: usize) {
		//let packets_tmp = Vec::new();
		for node in self.nodes.values_mut() {
			// Get incoming Packets
			let incoming_packets = self.router.tick_node(node.net_id());
			// Get outgoing Packets
			let outgoing_packets = node.tick(incoming_packets);
			// Send packets through the router
			self.router.add_packets(outgoing_packets);
		}
	}
}
