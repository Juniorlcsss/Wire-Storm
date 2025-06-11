//imports
use std::{io::{ErrorKind, IoSlice, Read, Write}, net::{Shutdown, TcpListener, TcpStream}, sync::{Arc,Mutex},thread,time::Duration};

//constants
const SRC_PORT: u16 = 33333;    //SEND_PORT in client.py    single send
const DST_PORT: u16 = 44444;    //RECV_PORT in client.py    multiple recv
const HEADER_LENGTH: usize= 8;  //HEADER_SIZE in tests.py
const MAGIC_BYTE: u8 = 0xCC;    //MAGIC_BYTE in tests.py    (byte-0 of every frame)
const SENSITIVE_BIT: u8 = 0x40;
const RESERVE_BIT: u8 = 0x80;
const PADDING_MASK: u8 = 0x3F;
const MAX_CAPACITY: usize =65536;

type SharedList = Arc<Mutex<Vec<TcpStream>>>;


#[inline(always)]
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

    calc_checksum(&total)==received_checksum
}

fn init_receiver(recievers:SharedList)->anyhow::Result<()>{
    let listener = TcpListener::bind(("0.0.0.0", DST_PORT))?;
    println!("Listening for recievers: {DST_PORT}");
    
    thread::spawn(move||{
        for connection in listener.incoming(){
            match connection{
                Ok(stream)=>{
                    let _ = stream.set_nodelay(true);
                    if let Ok(addr)=stream.peer_addr(){
                        println!("New receiver connected: {addr:?}");
                    }
                    recievers.lock().unwrap().push(stream);
                }
                Err(e) => eprintln!("Receiver accept error: {:?}",e),
            }
        }
    });
    Ok(())
}

#[inline(always)]
fn verify_header(hdr: &[u8]) -> Result<(u16, bool), &'static str> {
    //get info
    let length= u16::from_be_bytes([hdr[2], hdr[3]]);
    let options = hdr[1];
    let sensitive = (options & SENSITIVE_BIT)!=0;
    
    
    //check magic byte
    if hdr[0]!= MAGIC_BYTE{
        return Err("Invalid magic byte");
    }
    //check reserve bit
    if(options & RESERVE_BIT)!=0{
        return Err("Reserve bit must be 0");
    }
    //check padding bits
    if(options & PADDING_MASK) !=0{
        return Err("Invalid padding");
    }
    //check padding based on message type
    if !sensitive && hdr[4..8]!=[0x00;4]{
        return Err("Invaid padding for non sensitive");
    }
    else if sensitive && hdr[6..8] != [0x00; 2]{
        return Err("Invaid padding for sensitive");
    }

    Ok((length,sensitive))

}

fn handle_source(mut src:TcpStream, receivers: SharedList){
    /*
        Read CTMP frames from a single source and then broadcast
    */
    println!("Source connection from {:?}", src.peer_addr());
    let _ =src.set_nodelay(true);
    let _ = src.set_read_timeout(Some(Duration::from_secs(30)));
    let mut head_buffer = [0u8; HEADER_LENGTH];
    let mut body_buffer = Vec::with_capacity(MAX_CAPACITY);
    
    loop{
        //read header
        match src.read_exact(&mut head_buffer){
            Ok(_)=>{}
            Err(e)=>{
                if e.kind()!=ErrorKind::UnexpectedEof{
                    eprintln!("Error reading header {e:?}");
                }
                break;
            }
        }

        //check header and get info
        let(length, sensitive) = match verify_header(&head_buffer){
            Ok(result)=>result,
            Err(msg)=>{
                eprintln!("{msg:?}");
                let mut discard = vec![0u8;u16::from_be_bytes([head_buffer[2], head_buffer[3]]) as usize];
                if src.read_exact(&mut discard).is_err(){
                    break;
                }
                continue;
            }
        };
        
        //read body
        body_buffer.clear();
        body_buffer.resize(length as usize,0);
        if let Err(e)=src.read_exact(&mut body_buffer){
            eprintln!("Closed whilst reading body: ({e:?})");
            break;
        }

        //validate checksum
        if sensitive{
            let checksum = u16::from_be_bytes([head_buffer[4], head_buffer[5]]);
            if !verify_checksum(&head_buffer, &body_buffer, checksum){
                eprintln!("Invalid checksum");
                continue;
            }
        }

        //broadcast
        broadcast(&head_buffer, &body_buffer, receivers.clone());
    }

    let _ = src.shutdown(Shutdown::Both);
}


fn broadcast(header: &[u8], body: &[u8], receivers: SharedList){
    let mut receiver_list = receivers.lock().unwrap();
    let mut i=0;

    //prepare io slices
    let buffers = &[IoSlice::new(header),IoSlice::new(body)];

    while i < receiver_list.len(){
        match receiver_list[i].write_vectored(buffers){
            Ok(x) if x == header.len() + body.len()=>{
                i+=1;
            }
            _=>{
                eprintln!("Dropping reciever");
                let _ =receiver_list.swap_remove(i).shutdown(Shutdown::Both);
            }   
        }
    }
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