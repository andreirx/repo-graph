# Simple Makefile for integration testing
# Should produce 3 surfaces: build (library), test (cli), install (cli)

.PHONY: build test install clean

CC = gcc
CFLAGS = -Wall -O2

build: src/main.c src/utils.c
	$(CC) $(CFLAGS) -o app src/main.c src/utils.c

test: build
	./run_tests.sh

install: build
	cp app /usr/local/bin/

clean:
	rm -f app *.o
