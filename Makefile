UNAME := $(shell uname)

ifeq ($(UNAME), Linux)
NASMFLAGS += -felf64
endif
ifeq ($(UNAME), Darwin)
NASMFLAGS += -fmacho64
endif

all: runtime/stub.exe

runtime/stub.exe: runtime/stub.rs runtime/libcompiled_code.a
	rustc -Cforce-frame-pointers=yes -L runtime -o runtime/stub.exe runtime/stub.rs

runtime/libcompiled_code.a: runtime/compiled_code.o
	ar rus runtime/libcompiled_code.a runtime/compiled_code.o

runtime/compiled_code.o: runtime/compiled_code.s
	nasm $(NASMFLAGS) -o runtime/compiled_code.o runtime/compiled_code.s


clean:
	rm -f runtime/*.o runtime/*.a runtime/*.exe
