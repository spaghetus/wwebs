# wwebs - Willow's Web Server

wwebs is a CGI-first web server.

## how wwebs works

1. A request comes in.
2. Get the path of the request.
3. Walk up the request's path until...
  * The path hits a file before its end. If the file is executable, use it. Otherwise, return 404.
  * The path hits a directory at its end. Use the index.
  * The path hits a file or directory without the "others read" permission bit set. Return 404.
  * The path misses at any point. Return 404.
  * The path hits its end.
4. At every step of the path, check for `.wwebs.toml`, `.logger#`, `.gatekeeper#`, `.req_transformer#`, `.res_transformer#`
5. Execute all of the gatekeepers, in ascending order first by depth, then by number. If any of them fail, skip to step 8, executing only response transformers as deep or shallower than the gatekeeper that failed.
6. Execute all of the request transformers, in ascending order first by depth, then by number.
7. Execute the target file, if it is executable, otherwise read it into the response body.
8. Execute all of the response transformers, first in descending order by depth, then in ascending order by number.
9. Send the response.