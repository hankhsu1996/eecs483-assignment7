#[repr(C)]
#[derive(PartialEq, Eq, Copy, Clone)]
struct SnakeVal(u64);

#[repr(u64)]
pub enum SnakeErr {
    Overflow = 0,
    ArithExpectedNum = 1,
    CmpExpectedNum = 2,
    LogExpectedBool = 3,
    IfExpectedBool = 4,
}

static BOOL_TAG: u64 = 0x00_00_00_00_00_00_00_01;
static SNAKE_TRU: SnakeVal = SnakeVal(0xFF_FF_FF_FF_FF_FF_FF_FF);
static SNAKE_FLS: SnakeVal = SnakeVal(0x7F_FF_FF_FF_FF_FF_FF_FF);
static SNAKE_PADDING: SnakeVal = SnakeVal(0x01_00_FF_FF_FF_FF_FF_FF);
static mut BOTTOM: u64 = 0;

#[link(name = "compiled_code", kind = "static")]
extern "C" {

    // The \x01 here is an undocumented feature of LLVM that ensures
    // it does not add an underscore in front of the name.
    #[link_name = "\x01start_here"]
    fn start_here(bottom: *mut u64) -> SnakeVal;
}

// reinterprets the bytes of an unsigned number to a signed number
fn unsigned_to_signed(x: u64) -> i64 {
    i64::from_le_bytes(x.to_le_bytes())
}

fn sprint_snake_val(x: SnakeVal) -> String {
    if x.0 & BOOL_TAG == 0 {
        // it's a number
        format!("{}", unsigned_to_signed(x.0) >> 1)
    } else if x == SNAKE_TRU {
        String::from("true")
    } else if x == SNAKE_FLS {
        String::from("false")
    } else {
        format!("Invalid snake value 0x{:x}", x.0)
    }
}

#[export_name = "\x01err"]
extern "C" fn err() {
    std::process::exit(1)
}

#[export_name = "\x01print_snake_val"]
extern "C" fn print_snake_val(v: SnakeVal) -> SnakeVal {
    println!("{}", sprint_snake_val(v));
    v
}

#[export_name = "\x01dbg"]
extern "C" fn dbg(stack: u64) {
    println!("Rsp: 0x{:x}", stack);
}

#[export_name = "\x01snake_error"]
extern "C" fn snake_error(ecode: SnakeErr, v1: SnakeVal) -> SnakeVal {
    match ecode {
        SnakeErr::Overflow => eprintln!("Operation overflowed"),
        SnakeErr::ArithExpectedNum => {
            eprintln!("arithmetic expected a number, got {}", sprint_snake_val(v1))
        }
        SnakeErr::CmpExpectedNum => {
            eprintln!("comparison expected a number, got {}", sprint_snake_val(v1))
        }
        SnakeErr::LogExpectedBool => {
            eprintln!("logic expected a boolean, got {}", sprint_snake_val(v1))
        }
        SnakeErr::IfExpectedBool => {
            eprintln!("if expected a boolean, got {}", sprint_snake_val(v1))
        }
    }
    std::process::exit(1)
}

#[export_name = "\x01print_stack"]
extern "C" fn print_snake_stack(
    v: SnakeVal,
    frame_ptr: *const u64,
    stack_ptr: *const u64,
) -> SnakeVal {
    let mut base = frame_ptr;
    let mut p = stack_ptr;

    println!("BOTTOM: 0x{:x}", unsafe { BOTTOM });
    println!("stack_ptr: {:p}", stack_ptr);
    println!("frame_ptr: {:p}", frame_ptr);
    println!("LOCALS");
    loop {
        while p != base {
            let val = SnakeVal(unsafe { *p });
            println!("{0:p} ==> {1}", p, sprint_snake_val(val));
            p = unsafe { p.add(1) };
        }
        println!("{0:p} ==> 0x{1:x} BASE POINTER (saved rbp)", p, unsafe {
            *p
        });
        if p >= unsafe { BOTTOM as *const u64 } {
            break;
        }
        base = unsafe { *p as *const u64 };
    }
    return v;
}

fn main() {
    let output = unsafe { start_here(&mut BOTTOM) };
    println!("{}", sprint_snake_val(output));
}
