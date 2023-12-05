use libp2p::Swarm;
use libp2p::{gossipsub, mdns, swarm::NetworkBehaviour, tcp, tls, yamux};
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::{collections::hash_map::DefaultHasher, error::Error};
use tokio::io;

/// The `NetworkBehaviour` trait specifies the behaviour of the nodes
/// in the peer-to-peer network in all the situation/events that might
/// occur. Take a look at the source code such trait for more.
///
/// Here, both `gossipsub` and `mdns` implement the `NetworkBehaviour` trait.
///
/// We create a custom network behaviour that combines:
/// * Gossipsub: a type of publish-subscribe (to topic) protocol
/// * mDNS (multicast Domain Name System): is a way for nodes to use IP
/// multicast (a method used in computer networking to send Internet Protocol
/// (IP) datagrams to a group of interested receivers in a single transmission)
/// to publish and receive DNS records within a local network. In libp2p, mDNS
/// is used for peer discovery, allowing peers to find each other on the same
/// local network (e.g., your wi-fi) without any configuration
///
/// References:
/// * publish-subscribe, gossip: https://docs.libp2p.io/concepts/pubsub/overview/
/// * mDNS: https://docs.libp2p.io/concepts/discovery-routing/mdns/
#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
}

pub fn build_swarm() -> Result<Swarm<MyBehaviour>, Box<dyn Error>> {
    // Called also "switch", see documentation https://docs.libp2p.io/concepts/multiplex/switch
    // and also `libp2p::swarm` docs. The swarm contains the state of the network as a whole
    //
    // with_new_identity creates a new identity for the
    // local node generating a peer id
    let swarm = libp2p::SwarmBuilder::with_new_identity()
        // specifies the asynchronous runtime
        .with_tokio()
        // Next up we need to construct a transport. Each transport in libp2p provides encrypted streams.
        // E.g. combining TCP to establish connections, TLS to encrypt these connections and Yamux
        // to run one or more streams on a connection. Another libp2p transport is QUIC,
        // providing encrypted streams out-of-the-box. We will stick to TCP for now.
        // Each of these implement the Transport trait.
        .with_tcp(
            tcp::Config::default(),
            tls::Config::new,
            yamux::Config::default,
        )?
        // The .with_behaviour() method is used to specify the behavior of the nodes in the peer-to-peer network.
        // In libp2p, a NetworkBehaviour defines how nodes react to events and communicate with each other.
        // It's essentially the logic that governs the network interactions.
        .with_behaviour(|key| {
            // key is the cryptographic key-pair that identifies the node

            // To content-address message, we can take the hash of message and use it as an ID.
            let message_id_fn = |message: &gossipsub::Message| {
                println!("inside message_id_fn, message = {:?}", message);
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Set a custom gossipsub configuration
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                // see https://docs.libp2p.io/concepts/pubsub/overview/#grafting-and-pruning
                // how frequent to perform a check of grafting or pruning connections
                //
                // This is set a bit high to aid debugging by not cluttering the log space
                .heartbeat_interval(Duration::from_secs(10))
                // This sets the kind of message validation. The default is Strict (enforce message signing)
                .validation_mode(gossipsub::ValidationMode::Strict)
                // content-address messages. No two messages of the same content will be propagated.
                .message_id_fn(message_id_fn)
                .build()
                .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

            // build a gossipsub network behaviour
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
            Ok(MyBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}
