use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::collections::VecDeque;
use std::process;

#[derive(Clone)]
struct Process {
    name: String,       // process name
    pid: u32,       // process ID
    ppid: u32,      // parent process ID
    status: String,     // process status: S(sleeping) / W(waiting) / None
    lines: VecDeque<String>     // process file의 명령어 저장 queue
}

struct Sleeping {
    process: Process,       // sleep 중인 process 구조체
    cycle: u32              // sleep 잔여 cycle
}

static mut CYCLE: u32 = 0;
static mut PID: u32 = 1;
static mut MODE: String = String::new();       // user or kernel
static mut COMMAND: String = String::new();
static mut RQ: VecDeque<Process> = VecDeque::new();       // ready queue
static mut WQ: VecDeque<Process> = VecDeque::new();       // waiting queue
static mut SQ: VecDeque<Sleeping> = VecDeque::new();       // sleeping queue (sleep 중인 모든 process 저장)
static mut RUNNING: Option<Process> = None;        // 현재 실행 중인 process
static mut NEWP: Option<Process> = None;       // 새로 들어온 process 
static mut TERMINATED: Option<Process> = None;     // terminated 상태인 process 
static mut CYCLE_INFO: String = String::new();      // result 파일에 출력할 cycle 정보
static mut CYCLE_DONE: bool = false;     // cycle이 끝나고 결과를 출력해야하면 true / 아직 출력할 때가 아니면 false
static mut INPUT_DIR: String = String::new();       // 가상 프로그램들이 들어있는 폴더 경로 저장

// 새로운 process 만들고 return하는 함수
fn create_process(name: String, pid: u32, ppid: u32, status: String, lines: VecDeque<String>) -> Process {
    Process {
        name,
        pid,
        ppid,     
        status,
        lines,
    }
}

// 매 cycle에 관한 정보 CYCLE_INFO에 추가하는 함수
fn print_cycle()
{
    unsafe{
        let mut temp: String;
        if !CYCLE_DONE {return;}
        else
        {
            temp = format!("[cycle #{CYCLE}]\n1. mode: {MODE}\n2. command: {COMMAND}\n");
            // 3. running 출력
            match &RUNNING {
                None => temp.push_str("3. running: none\n"),
                Some(p) => temp.push_str(&format!("3. running: {}({}, {})\n", p.pid, p.name, p.ppid))
            }
            // 4. ready 출력
            if RQ.is_empty() {
                temp.push_str("4. ready: none\n");
            } else {
                temp.push_str("4. ready: ");
                for p in &RQ{
                    temp.push_str(&format!("{} ", p.pid));
                }
                temp.push_str("\n");
            }
            // 5. waiting 출력
            if WQ.is_empty() {
                temp.push_str("5. waiting: none\n");
            } else {
                temp.push_str("5. waiting: ");
                for p in &WQ{
                    temp.push_str(&format!("{}({}) ", p.pid, p.status));
                }
                temp.push_str("\n");
            }
            // 6. new 출력
            match &NEWP {
                None => temp.push_str("6. new: none\n"),
                Some(p) => temp.push_str(&format!("6. new: {}({}, {})\n", p.pid, p.name, p.ppid))
            }
            // 7. terminated 출력
            match &TERMINATED {
                None => temp.push_str("7. terminated: none\n\n"),
                Some(p) => temp.push_str(&format!("7. terminated: {}({}, {})\n\n", p.pid, p.name, p.ppid))
            }
        }
        CYCLE_INFO.push_str(&temp);
    }
}
// schedule 함수
fn idle_or_schedule()
{
    unsafe{
        MODE = String::from("kernel");
        CYCLE += 1;     // 1 cycle 소비
        if !RUNNING.is_none() {return;}     // 이미 running 상태의 process가 있다면 스케줄 필요X
        else {
            match RQ.pop_front() {
                None => {
                    COMMAND = String::from("idle");     // ready queue is empty
                    CYCLE_DONE = true;
                    print_cycle();
                    return;
                }
                Some(p) => {
                    COMMAND = String::from("schedule");
                    RUNNING = Some(p);       // ready queue의 첫번째 process를 running으로
                    CYCLE_DONE = true;
                    print_cycle();
                    running_process();      // 다음 프로세스 진행
                }
            }
        }
    }
}
// 명령어 fork 처리
fn fork_and_exec(name: String) {
    unsafe{
        // 1. fork 명령어가 실행된 첫 번째 cycle 출력 
        CYCLE += 1;
        sleep_minus_one();
        COMMAND = String::from(format!("fork_and_exec {name}"));
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");
        
        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        sleep_minus_one();
        COMMAND = String::from("system call");
        match &RUNNING {
            None => return,
            Some(running) => {
                // 새로운 process 생성
                    // 새로 들어온 process를 읽고 한 줄씩 VecDeque에 저장
                let process_dir: String = format!("{}\\{}", INPUT_DIR, name).to_string();
                let mut lines: VecDeque<String> = VecDeque::new();
                let file = File::open(process_dir).unwrap();
                let reader = BufReader::new(file).lines();
                for line in reader {
                    lines.push_back(line.unwrap());
                }
                PID += 1;
                let p = create_process(name, PID, running.pid, "None".to_string(), lines);      // 새로운 process의 부모는 현재 running process
                NEWP = Some(p);     // new process 갱신
                RQ.push_back(running.clone());      // 부모 process(현재 running process) ready queue에 넣기
                RUNNING = None;
            }
        }
        CYCLE_DONE = true;
        print_cycle();

        // 3. 세 번째 cycle 출력
        sleep_minus_one();
            // new 상태의 process ready queue에 넣기
        match &NEWP {
            None => return,
            Some(p) => {
                RQ.push_back(p.clone());
                NEWP = None;
            }
        } 
        idle_or_schedule();     // scheduling
        running_process();      // 다음 프로세스 명령어 실행
    }
}
// 명령어 run 처리
fn run(arg: u32) {
    unsafe{
        COMMAND = String::from(format!("run {arg}"));
        for _ in 0..arg {
            CYCLE += 1;
            sleep_minus_one();
            CYCLE_DONE = true;
            print_cycle();
        }
    }
}
// 명령어 sleep 처리
fn sleep(arg: u32) {
    unsafe {
        // cycle #1
        CYCLE += 1;
        sleep_minus_one();     
        COMMAND = String::from(format!("sleep {}", arg));
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // mode switching

        // cycle #2
        CYCLE += 1;
        COMMAND = String::from("system call");      
        match &RUNNING {
             None => return,
             Some(p) => {
                let c = p.clone();
                // running -> waiting
                WQ.push_back(create_process(c.name, c.pid, c.ppid, "S".to_string(), c.lines));
                let s = Sleeping{
                    process: p.clone(),
                    cycle: arg,
                };
                // sleeping queue에도 넣어주기
                SQ.push_back(s);
             }
        }
        RUNNING = None;     // 이걸 추가했더니 memory allocation failed가 뜸
        sleep_minus_one(); 
        CYCLE_DONE = true;
        print_cycle();

        // cycle #3
        sleep_minus_one();  
        if !RQ.is_empty() {
            idle_or_schedule();
            return;     // 다음 프로세스가 스케줄링 된 경우 sleep 함수 마침
        } else {
            idle_or_schedule();     // 결과: idle
        }
        
        // arg > 2일 때
        // cycle #4
        while RQ.is_empty() {
            sleep_minus_one();
            if !RQ.is_empty() {
                idle_or_schedule();
                return;
            }
            idle_or_schedule();
        }
        return;
    }
}
// sleep 중인 모든 process의 잔여 cycle - 1 하고, 잔여 cycle = 0이 된 process는 ready 시키기
fn sleep_minus_one() {
    unsafe{
        if SQ.is_empty() {return;}
        let mut i = 0;
        for mut s in &mut SQ {
            s.cycle -= 1;       // 모든 sleep process의 잔여 cycle - 1
            if s.cycle == 0 {       // 잔여 cycle = 0이 됨
                RQ.push_back(s.process.clone());
                // Waiting queue에서 해당 process 찾아서 없애기
                for (index, value) in WQ.iter_mut().enumerate() {
                    if value.pid == s.process.pid {
                        WQ.remove(index);
                        break;
                    }
                }
                SQ.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

// 명령어 wait 처리
fn wait() {
    unsafe{
        // 1. 첫 번째 cycle 출력
        CYCLE += 1;
        sleep_minus_one();
        COMMAND = String::from("wait");
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // 모드 스위칭

        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        sleep_minus_one();
        COMMAND = String::from("system call");
            // ready queue에 자식 프로세스가 존재하는지 확인
        let mut find = false;
        match &RUNNING {
            None => return,
            Some(p) => {
                for (_, value) in RQ.iter_mut().enumerate() {
                    if value.ppid == p.pid {      // 자식 프로세스 존재
                        let p1 = p.clone();
                        WQ.push_back(create_process(p1.name, p1.pid, p1.ppid, "W".to_string(), p1.lines));
                        find = true;
                        break;
                    }
                }
                if !find {      // 자식 프로세스 없음
                    RQ.push_back(p.clone());
                }
                RUNNING = None;
                CYCLE_DONE = true;
                print_cycle();
            }
        }

        // 3. 세 번째 cycle 출력 
        sleep_minus_one();
        idle_or_schedule();
    }
}
// 명령어 exit 처리
fn exit() {
    unsafe{
        // 1. 첫 번째 cycle 출력
        CYCLE += 1;
        sleep_minus_one();
        COMMAND = String::from("exit");
        CYCLE_DONE = true;
        print_cycle();
        MODE = String::from("kernel");      // 모드 스위칭

        // 2. 두 번째 cycle 출력
        CYCLE += 1;
        sleep_minus_one();
        COMMAND = String::from("system call");
        match &RUNNING {
            None => return,
            Some(c) => {
                // 부모 process가 waiting 중인지 확인
                for (index, value) in WQ.iter_mut().enumerate() {
                    if value.pid == c.ppid {
                        RQ.push_back(value.clone());
                        WQ.remove(index);
                        break;
                    }
                }
                TERMINATED = Some(c.clone());
                RUNNING = None;
            }
        }
        CYCLE_DONE = true;
        print_cycle();

        // 3. 세 번째 cycle 출력
        TERMINATED = None;
        match &NEWP {
            Some(_) => {            // 종료되지 않은 new process가 존재할 경우
                sleep_minus_one();
                idle_or_schedule();      
            },
            None => {
                if RQ.is_empty() && WQ.is_empty() {     // 종료되지 않은 프로세스가 running process 단 하나일 경우
                    return;
                } else {        // 종료되지 않은 프로세스가 더 남아있는 경우
                    sleep_minus_one();
                    idle_or_schedule();
                }
            }
        }
    }
}

// 프로그램 파일 읽고 명령어에 맞게 처리하는 함수
fn running_process() {
    unsafe{
        match &RUNNING {
            None => return,
            Some(p) => {
                let mut v = p.lines.clone();
                while !v.is_empty() {
                    MODE = String::from("user");
                    let order = v.pop_front().unwrap();
                    if order.contains("run") {      // 명령어 run이 들어왔을 경우
                        let n: u32 = order.split(" ").last().unwrap().parse().unwrap();
                        run(n);
                    } else if order.contains("fork_and_exec") {       // 명령어 fork가 들어왔을 경우
                        // running process의 요소 lines를 이후 남은 명령어들의 queue로 갱신해주기
                        let mut new_lines: VecDeque<String> = VecDeque::new();
                        for after in &v {
                            new_lines.push_back(after.to_string());
                        }
                        let c = p.clone();
                        RUNNING = Some(create_process(c.name, c.pid, c.ppid, "None".to_string(), new_lines));
                        // fork 해줄 process의 이름 추출하고 fork 명령 실행
                        let name = order.split(" ").last().unwrap().to_string();
                        fork_and_exec(name);
                        return;
                    } else if order.contains("sleep") {     // 명령어 sleep이 들어왔을 경우
                        // running process의 요소 lines를 이후 남은 명령어들의 queue로 갱신해주기
                        let mut new_lines: VecDeque<String> = VecDeque::new();
                        for after in &v {
                            new_lines.push_back(after.to_string());
                        }
                        let c = p.clone();
                        RUNNING = Some(create_process(c.name, c.pid, c.ppid, "None".to_string(), new_lines));
                        // sleep 해줄 cycle 개수를 추출하고 sleep 명령 실행
                        let n: u32 = order.split(" ").last().unwrap().parse().unwrap();
                        sleep(n);
                        return;
                    } else if order.contains("wait") {      // 명령어 wait가 들어왔을 경우
                        // running process의 요소 lines를 이후 남은 명령어들의 queue로 갱신해주기
                        let mut new_lines: VecDeque<String> = VecDeque::new();
                        for after in &v {
                            new_lines.push_back(after.to_string());
                        }
                        let c = p.clone();
                        RUNNING = Some(create_process(c.name, c.pid, c.ppid, "None".to_string(), new_lines));
                        wait();
                        return;
                    } else if order.contains("exit") {      // 명령어 exit가 들어왔을 경우
                        exit();
                        return;
                    }
                     else {
                        println!("wrong order!");
                    }
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();      
    unsafe{
        INPUT_DIR = String::from(&args[1]);    // input file들이 있는 폴더 경로 저장    
        // cycle #0
            // init 생성 
            // int 프로그램 파일 읽고 한 줄씩 VecDeque에 저장
        let process_dir: String = format!("{}\\{}", INPUT_DIR, "init").to_string();
        let mut lines: VecDeque<String> = VecDeque::new();
        let file = File::open(process_dir).unwrap();
        let reader = BufReader::new(file).lines();
        for line in reader {
            lines.push_back(line.unwrap());
        }
        let init = create_process("init".to_string(), PID, 0, "None".to_string(), lines);
        MODE = String::from("kernel");
        COMMAND = String::from("boot");
        NEWP = Some(init);
        CYCLE_DONE = true;
        print_cycle();

        // cycle #1
            // new process -> ready queue
        match &NEWP {
            None => return,
            Some(p) => {
                RQ.push_back(p.clone());
                NEWP = None;
            }
        }
        idle_or_schedule();     // ready -> running

        // cycle #2~ 
        running_process();

        let mut result = std::fs::File::create("result").expect("create failed");
        result.write_all(CYCLE_INFO.as_bytes()).expect("write failed");
        println!("result written to file" );
        process::exit(1);
    }
}
