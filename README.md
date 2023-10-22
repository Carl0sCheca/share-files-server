This server works together with [share-files-client](https://github.com/Carl0sCheca/share-files-client)

You can also upload files using curl:

```bash
curl -X POST -H "format:file_format" -H "share-token:token" --data-binary "@/path/to/file" server_url
```
