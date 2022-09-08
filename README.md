# wwebs - Willow's Web Server

wwebs is a weird (but hopefully not too weird) webserver.

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

## writing dynamic content

For content to be dynamic, it must have the `o+r` and `o+x` permission bits. Dynamic content is a normal executable file.

Dynamic content receives the following information:
* `/dev/stdin` - The request body, if applicable.
* `HEADER_*` - The request headers.
* `QUERY_*` - The query strings.
* `VERB` - The verb of the request.
* `REQUESTED` - The full URL of the request.
* `STATUS` - The status code of the response, if this content handles responses.

Dynamic content generates the following information:
* `/dev/stdout` - The response body.
* `/dev/stderr` - Output commands.
  * `status ###` - Set the status.
  * `log ...` - Write a logging message.