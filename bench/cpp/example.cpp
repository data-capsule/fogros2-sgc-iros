// Client side C/C++ program to demonstrate Socket
// programming
#include <arpa/inet.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
#define PORT 31415

struct gdp {
    char src_gdpname[32];
    char dst_gdpname[32]; 
    char uuid[16]; 
    char num_packets[4];  // 32 / 8 
    char packet_no[4];  // 32 / 8 
    char data_len[2];  // 16 / 8 
    char action;  // 8 / 8 
    char ttl[1];  // 8 / 8 
    char payload; 
} gdp_header;


int main(int argc, char const* argv[])
{
	int sock = 0, valread, client_fd;
	struct sockaddr_in serv_addr;
	char* hello = "Hello from client";
	struct gdp h = {0};
	h.action = (char) 5;
	h.payload = (char) 65;
	
	char buffer[1024] = { 0 };
	if ((sock = socket(AF_INET, SOCK_DGRAM, 0)) < 0) {
		printf("\n Socket creation error \n");
		return -1;
	}

	serv_addr.sin_family = AF_INET;
	serv_addr.sin_port = htons(PORT);

	// Convert IPv4 and IPv6 addresses from text to binary
	// form
	if (inet_pton(AF_INET, "128.32.37.48", &serv_addr.sin_addr)
		<= 0) {
		printf(
			"\nInvalid address/ Address not supported \n");
		return -1;
	}

	if ((client_fd
		= connect(sock, (struct sockaddr*)&serv_addr,
				sizeof(serv_addr)))
		< 0) {
		printf("\nConnection Failed \n");
		return -1;
	}
    while(true){
    	send(sock, (void*) &h, sizeof(h), 0);
	    printf("Hello message sent\n");
        sleep(1);
    }
	valread = read(sock, buffer, 1024);
	printf("%s\n", buffer);

	// closing the connected socket
	close(client_fd);
	return 0;
}
