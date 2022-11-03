CC=g++
CFLAGS=-I. -std=c++17
CLINK=-lzmq -lstdc++
DEPS = example.cpp

all: example.o 
	$(CC) -o example example.o $(CLINK)
example.o: 
	$(CC) -c -o example.o $(CFLAGS) example.cpp
run:
	./example
clean: 
	rm -f example.o example
