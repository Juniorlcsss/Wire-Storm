//imports
use std::{io::{Read,Write}, net::{Shutdown, TcpListener, TcpStream}, sync::{Arc,Mutex},thread,time::Duration};

//constants
const SRC_PORT: u16 = 33333;    //SEND_PORT in client.py    single send
const DST_PORT: u16 = 44444;    //RECV_PORT in client.py    multiple recv
const HEADER_LENGTH: usize= 8;  //HEADER_SIZE in tests.py
const MAGIC_BYTE: u8 = 0xCC;    //MAGIC_BYTE in tests.py    (byte-0 of every frame)

type SharedList = Arc<Mutex<Vec<TcpStream>>>;


fn init_receiver(recievers:SharedList)->anyhow::Result<()>{
    let listener = TcpListener::bind(("0.0.0.0", DST_PORT))?;
    println!("Listening for recievers: {DST_PORT}");
    
    thread::spawn(move||{
        for connection in listener.incoming(){
            match connection{
                Ok(stream)=>{
                    let _ = stream.set_nodelay(true);
                    println!("New receiver connected: {}",stream.peer_addr().unwrap());
                    recievers.lock().unwrap().push(stream);
                }
                Err(e) => eprintln!("Receiver accept error: {:?}",e),
            }
        }
    });
    Ok(())
}

fn handle_source(mut src:TcpStream, receivers: SharedList){
    /*
        Read CTMP frames from a single source and then broadcast
    */
    println!("Source connection from {:?}", src.peer_addr());
    let _ =src.set_nodelay(true);
    let _ = src.set_read_timeout(Some(Duration::from_secs(30)));
    
    loop{
        //read header
        let mut hdr = [0u8; HEADER_LENGTH];
        if let Err(e)= src.read_exact(&mut hdr){
            eprintln!("Source closed ({e:?})");
            break;
        }

        //get length
        let length = u16::from_be_bytes([hdr[2], hdr[3]]) as usize;

        //validate magic byte and padding
        if hdr[0] != MAGIC_BYTE || hdr[1] != 0x00 || hdr[4..8] != [0x00; 4] {
            eprintln!("magic or padding check failed");
            let mut discard = vec![0u8;length];
            if let Err(e)=src.read_exact(&mut discard){
                eprintln!("Failed to discard body: {e:?}");
                break;
            }
            continue;
        }


        //read body
        let mut body = vec![0u8; length];
        if let Err(e)=src.read_exact(&mut body){
            eprintln!("Closed whilst reading body: ({e:?})");
            break;
        }

        //compose frame
        let mut frame = Vec::with_capacity(HEADER_LENGTH+length);
        frame.extend_from_slice(&hdr);
        frame.extend_from_slice(&body);

        //broadcast
        let mut receiver_list = receivers.lock().unwrap();
        let mut i = 0;

        while i < receiver_list.len(){
            match receiver_list[i].write_all(&frame){
                Ok(_) =>{
                    let _ = receiver_list[i].flush();
                    i+=1;
                }
                Err(_)=>{
                    //remove reciever
                    eprintln!("Dropping reciever");
                    let _ = receiver_list.swap_remove(i).shutdown(Shutdown::Both);
                }
            }
        }
    }
    let _ = src.shutdown(Shutdown::Both);
}


fn main()->anyhow::Result<()>{
    //set one thread accepting new reciever client
    let recievers: SharedList = Arc::new(Mutex::new(Vec::new()));
    init_receiver(recievers.clone())?;

    //wait for src
    let src_listener = TcpListener::bind(("0.0.0.0", SRC_PORT))?;
    println!("Listening for CTMP on port: {}",SRC_PORT);

    //get incoming traffic
    for src in src_listener.incoming(){
        match src{
            Ok(stream)=>{
                println!("Connected to source");
                let receive = recievers.clone();
                thread::spawn(move || handle_source(stream, receive));
            }
            //debug
            Err(e)=> eprintln!("Error accepting source: {e:?}"),
        }
    }
    Ok(())
}