#+build darwin, linux
package main

import socket "../agent/socket"
import "core:sys/posix"
import "core:time"

run_proxy :: proc(endpoint:string)->bool {
    channel,err:=socket.connect(endpoint); if err!=.None { return false }; defer socket.close(&channel)
    _,_,buffer_error:=socket.buffer_prepare(&channel,false); if buffer_error!=.None { return false }
    input_fd:=posix.FD(0); output_fd:=posix.FD(1)
    input_flags:=posix.fcntl(input_fd,.GETFL); output_flags:=posix.fcntl(output_fd,.GETFL)
    if input_flags<0 || output_flags<0 { return false }
    if posix.fcntl(input_fd,.SETFL,input_flags|posix.O_NONBLOCK)<0 { return false }
    defer posix.fcntl(input_fd,.SETFL,input_flags)
    if posix.fcntl(output_fd,.SETFL,output_flags|posix.O_NONBLOCK)<0 { return false }
    defer posix.fcntl(output_fd,.SETFL,output_flags)
    signals:posix.sigset_t; posix.sigemptyset(&signals); posix.sigaddset(&signals,posix.Signal(posix.SIGPIPE)); posix.pthread_sigmask(.BLOCK,&signals,nil)
    input,output:[65536]byte
    input_count,input_offset,output_count,output_offset:int
    stdin_closed,write_closed,socket_closed:bool
    last_progress:=time.tick_now()
    for {
        progress:=false
        if !stdin_closed && input_count==0 && !socket_closed {
            p:=posix.pollfd{fd=input_fd,events={.IN}}
            if posix.poll(&p,1,0)<0 && posix.errno()!=.EINTR { return false }
            if .IN in p.revents || .HUP in p.revents {
                n:=posix.read(input_fd,&input[0],len(input))
                if n==0 { stdin_closed=true; progress=true }
                else if n>0 { input_count=int(n); input_offset=0; progress=true }
                else if posix.errno()!=.EAGAIN && posix.errno()!=.EINTR { return false }
            }
        }
        if input_count>0 && !socket_closed {
            n,write_error:=socket.write(&channel,input[input_offset:input_count])
            if write_error!=.None { return false }; input_offset+=n; progress=progress || n>0
            if input_offset==input_count { input_count=0; input_offset=0 }
        }
        if stdin_closed && input_count==0 && !write_closed && !socket_closed {
            if socket.shutdown_write(&channel)!=.None { return false }; write_closed=true; progress=true
        }
        if output_count==0 && !socket_closed {
            ready,wait_error:=socket.wait(&channel,false,0)
            if wait_error==.Closed { socket_closed=true; progress=true }
            else if wait_error!=.None { return false }
            else if ready {
                n,read_error:=socket.read(&channel,output[:])
                if read_error==.Closed { socket_closed=true; progress=true }
                else if read_error!=.None { return false }
                else if n>0 { output_count=n; output_offset=0; progress=true }
            }
        }
        if output_count>0 {
            p:=posix.pollfd{fd=output_fd,events={.OUT}}
            if posix.poll(&p,1,0)<0 && posix.errno()!=.EINTR { return false }
            if .ERR in p.revents || .HUP in p.revents || .NVAL in p.revents { return false }
            if .OUT in p.revents {
                n:=posix.write(output_fd,&output[output_offset],uint(output_count-output_offset))
                if n<0 && posix.errno()!=.EAGAIN && posix.errno()!=.EINTR { return false }
                if n>0 { output_offset+=int(n); progress=true }
                if output_offset==output_count { output_count=0; output_offset=0 }
            }
        }
        if socket_closed && output_count==0 { return input_count==0 }
        if progress { last_progress=time.tick_now() }
        else {
            if (input_count>0 || output_count>0) && time.tick_since(last_progress)>15*time.Second { return false }
            time.sleep(time.Millisecond)
        }
    }
}
