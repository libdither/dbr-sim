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

pub trait CustomNode: std::fmt::Debug {
	type CustomNodeAction;
	fn net_id(&self) -> InternetID;
	fn tick(&mut self, incoming: Vec<InternetPacket>) -> Vec<InternetPacket>;
	fn action(&mut self, action: Self::CustomNodeAction);
}

/// Internet router
#[derive(Default, Debug)]
pub struct InternetRouter {
	/// Map linking Node pairs to speed between them (supports differing 2-way speeds)
	speed_map: HashMap<(InternetID, InternetID), usize>,
	/// Map linking destination `Node`s to inbound packets
	packet_map: HashMap<InternetID, Vec<(InternetPacket, usize)>>,
}
impl InternetRouter {
	fn add_packets(&mut self, packets: Vec<InternetPacket>) {
		for packet in packets {
			let index = (packet.src_addr, packet.dest_addr);

			// Get Latency from speed map
			let latency = if let Some(speed) = self.speed_map.get(&index) {
				*speed
			} else {
				// TODO: precalculate speed from some node distance function
				let random_speed = rand::random::<usize>() % 10 + 1;
				self.speed_map.insert(index, random_speed);
				self.speed_map.insert((index.1, index.0), random_speed); // Need to add same speed the other direction
				random_speed
			};
			// Add packet to packet stream
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

#[derive(Debug)]
pub struct InternetSim<CN: CustomNode> {
	nodes: HashMap<InternetID, CN>,
	router: InternetRouter,
}
impl<CN: CustomNode> InternetSim<CN> {
	pub fn new() -> InternetSim<CN> {
		InternetSim {
			nodes: HashMap::new(),
			router: Default::default(),
		}
	}
	pub fn lease(&self) -> InternetID {
		self.nodes.len()
	}
	pub fn add_node(&mut self, node: CN) {
		self.nodes.insert(node.net_id(), node);
	}
	pub fn del_node(&mut self, net_id: InternetID) {
		self.nodes.remove(&net_id);
	}
	pub fn node_mut(&mut self, node_id: InternetID) -> Option<&mut CN> {
		self.nodes.get_mut(&node_id)
	}
	pub fn node(&self, node_id: InternetID) -> Option<&CN> {
		self.nodes.get(&node_id)
	}
	pub fn node_action(&mut self, node_id: InternetID, action: CN::CustomNodeAction) -> Result<(), Box<dyn std::error::Error>> {
		Ok( self.nodes.get_mut(&node_id).ok_or("internet: node_action: invalid NodeID")?.action(action) )
	}
	pub fn list_nodes(&mut self) {
		for (key, item) in &self.nodes {
			println!("	{:?}", item);
		}
	}
	pub fn run(&mut self, ticks: usize) {
		//let packets_tmp = Vec::new();
		for i in 0..ticks {
			for node in self.nodes.values_mut() {
				// Get Packets going to node
				let incoming_packets = self.router.tick_node(node.net_id());
				// Get packets coming from node
				if incoming_packets.len() > 0 { log::debug!("Node: {:?} Receiving {:?}", node.net_id(), incoming_packets); }
				let outgoing_packets = node.tick(incoming_packets);
				// Send packets through the router
				self.router.add_packets(outgoing_packets);
			}
		}
		
	}
}
