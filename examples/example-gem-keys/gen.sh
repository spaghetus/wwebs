#!/bin/bash
openssl genrsa -out localhost.key 2048
openssl req -new -sha256 -key localhost.key -out localhost.csr
openssl x509 -req -days 99999 -in localhost.csr -signkey localhost.key -out localhost.crt
openssl pkcs12 -export -in localhost.crt -inkey localhost.key -out localhost.p12 -name localhost