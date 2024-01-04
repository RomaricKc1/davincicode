use colored::Colorize;
use std::io::{self, Write};
use std::process;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Notify};
use tokio::task;
use tokio::time::{self, Duration};

use clap::Parser;

/// The client to the davinci code game
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// User name
    #[arg(short, long)]
    name: String,

    /// Server address
    #[arg(short, long)]
    addr: String,

    /// Server port
    #[arg(short, long)]
    port: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let address = format!("{}:{}", args.addr, args.port);
    let mut buffer = [0u8; 1024];

    let mut stream = TcpStream::connect(address.clone()).await.unwrap();
    println!("{} {}", "Connected to server at".green(), address);

    // send the "init" message after connecting
    let init_message = "init";
    stream.write_all(init_message.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let bytes_read = stream.read(&mut buffer).await.unwrap();
    let response = String::from_utf8_lossy(&buffer[..bytes_read]);
    println!("{} {}", "Response from server:".blue(), response);

    // send your name
    let name = args.name.trim();
    stream.write_all(name.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    println!("{}", "Sent name and init to server".green());

    // an Arc<Mutex<_>> to share the stream across tasks safely
    let shared_stream = Arc::new(Mutex::new(stream));
    let cloned_stream = Arc::clone(&shared_stream);

    // a Notify to signal the client when it's their turn to move
    let turn_notify = Arc::new(Notify::new());
    let cloned_notify = Arc::clone(&turn_notify);

    // spawn the listener task
    let receive_task = task::spawn(async move {
        let mut cloned_buffer = [0u8; 1024];

        loop {
            // println!("rx Waiting lock");
            let mut stream = cloned_stream.lock().await;
            // println!("rx aquired lock");

            let read_timeout =
                time::timeout(Duration::from_secs(1), stream.read(&mut cloned_buffer));

            match read_timeout.await {
                Ok(result) => {
                    let bytes_read = result.unwrap();
                    let response = String::from_utf8_lossy(&cloned_buffer[..bytes_read]);

                    println!("\n{} {}", "Response from server:".blue(), response);

                    // is it my turn?
                    if response.trim().contains("It's your turn") {
                        cloned_notify.notify_one();
                        // println!("Notified tx");
                    } else if response.trim().contains("You won! Congrats!") {
                        println!("{}\n", "Nice, exiting.".green());
                        process::exit(0);
                    } else {
                        //println!("not a cmd with rq see? ==> {}\n", response);
                    }

                    if bytes_read == 0 {
                        break;
                    }
                    // println!("Waiting for your turn.");
                }
                Err(_) => {
                    //println!("No response, moving on");
                }
            }
            drop(stream); // release the lock
        }
    });

    // main loop to send messages
    loop {
        turn_notify.notified().await;

        print!("<<< ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let mut stream = shared_stream.lock().await;

        stream.write_all(input.trim().as_bytes()).await.unwrap();
        stream.flush().await.unwrap();

        if input.trim() == "exit" {
            break;
        }
        println!("{}", ">>>".green());

        drop(stream); // releas the lock
    }

    // join
    receive_task.await.unwrap();
}
