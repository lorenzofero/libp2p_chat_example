use chat_example::{build_swarm, MyBehaviourEvent};
use futures::stream::StreamExt;
use libp2p::{gossipsub, mdns, swarm::SwarmEvent};
use std::error::Error;
use tokio::io::AsyncBufReadExt;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // it is an utility the for implementing and composing tracing subscribers
    // in this case traces are filtered using the `RUST_LOG` environment variable
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(LevelFilter::INFO.into()))
        .try_init();

    // build the swarm
    let mut swarm = build_swarm()?;

    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("test-topic");
    // subscribes to our topic
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    // Read full lines from stdin
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

    // Kick it off
    loop {
        // Look at the docs of the `select` macro
        tokio::select! {
            Ok(Some(line)) = stdin.next_line() => {
                // if there is some new input from stdin publish to everyone
                // subscribed to the topic we created. This will call the
                // `message_id_fn` to create an id for our message.
                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    tracing::info!("Publish error: {e:?}");
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        tracing::info!("mDNS discovered a new peer: {peer_id}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        tracing::info!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => tracing::info!(
                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data),
                    ),
                SwarmEvent::NewListenAddr { address, .. } => {
                    tracing::info!("Local node is listening on {address}");
                },
                // Adding a callback on a custom event is easy! Look
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Subscribed { peer_id, topic})) => {
                    tracing::info!("PeerId {peer_id} subscribed to topic {topic}");
                },
                _ => {}
            }
        }
    }
}
