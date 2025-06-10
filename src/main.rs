//imports
use std::{io::{Read,Write}, net::{Shutdown, TcpListener, TcpStream}, sync::{Arc,Mutex},thread,time::Duration};

//constants
const SRC_PORT: u16 = 33333;    //SEND_PORT in client.py    single send
const DST_PORT: u16 = 44444;    //RECV_PORT in client.py    multiple recv
const HEADER_LENGTH: usize= 8;  //HEADER_SIZE in tests.py
const MAGIC_BYTE: u8 = 0xCC;    //MAGIC_BYTE in tests.py    (byte-0 of every frame)
const SENSITIVE_BIT: u8 = 0x40;
const RESERVE_BIT: u8 = 0x80;

type SharedList = Arc<Mutex<Vec<TcpStream>>>;

fn calc_checksum(data: &[u8]) ->u16 {
    //calculate the ones complement checksum
    let mut sum: u32=0;

    //process 16-bit words
    let chunks = data.chunks_exact(2);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let string = u16::from_be_bytes([chunk[0], chunk[1]]);
        sum += string as u32;
    }

    //handle remainder
    if !remainder.is_empty(){
        let string = u16::from_be_bytes([remainder[0],0]);
        sum+= string as u32;
    }

    //add carry bits
    while(sum >>16)>0{
        sum = (sum & 0xFFFF) + (sum>>16);
    }

    //ones complement
    !sum as u16
}

fn verify_checksum(header: &[u8], body: &[u8], received_checksum: u16) -> bool {
    let mut check_head = header.to_vec();
    check_head[4] = MAGIC_BYTE;
    check_head[5] = MAGIC_BYTE;

    //combine header and body
    let mut total = Vec::with_capacity(check_head.len()+body.len());
    total.extend_from_slice(&check_head);
    total.extend_from_slice(body);

    let calc = calc_checksum(&total);
    calc == received_checksum
}

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

        //get info
        let length = u16::from_be_bytes([hdr[2], hdr[3]]) as usize;
        let options = hdr[1];
        let checksum = u16::from_be_bytes([hdr[4],hdr[5]]);
        let sensitive = (options & SENSITIVE_BIT)!=0;

        //validate magic byte
        if hdr[0] != MAGIC_BYTE {
            eprintln!("magic or padding check failed");
            let mut discard = vec![0u8;length];
            if let Err(e)=src.read_exact(&mut discard){
                eprintln!("Failed to discard body: {e:?}");
                break;
            }
            continue;
        }

        //validate options
        if (options & RESERVE_BIT) !=0 || (options &0x3F) != 0 {
            eprintln!("invalid options");
            let mut discard = vec![0u8;length];
            if let Err(e)=src.read_exact(&mut discard){
                eprintln!("Failed to discard body: {e:?}");
                break;
            }
            continue;
        }

        //non sensitive
        if !sensitive && (hdr[4..8] != [0x00; 4]) {
            eprintln!("Invalid padding for non-sensitive message");
            let mut discard = vec![0u8;length];
            if let Err(e)=src.read_exact(&mut discard){
                eprintln!("Failed to discard body: {e:?}");
                break;
            }
            continue;
        }

        //sensitive
        if sensitive && (hdr[6..8] != [0x00; 2]) {
            eprintln!("Invalid padding bytes");
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

        //validate checksum
        if sensitive{
            if !verify_checksum(&hdr, &body, checksum){
                eprintln!("Invalid checksum");
                continue;
            }
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