# zeph

An interactive CLI tool to quickly and easily inspect zarr stores. Supports local and remote (S3, GCS, Azure, HTTPS) stores. 

## Commands

| Command | Description |
| --- | --- |
| [`/summary`](https://github.com/tomwetherell/zeph?tab=readme-ov-file#summary) | - |
| [`/info`](https://github.com/tomwetherell/zeph?tab=readme-ov-file#info) | - |

### summary

<img width="1177" height="626" alt="image" src="https://github.com/user-attachments/assets/a5213049-fb00-4cbd-a87e-c0917db8995c" />

### info 

<img width="1177" height="610" alt="image" src="https://github.com/user-attachments/assets/fde349b3-f594-411a-a602-8088c1de3438" />

## Limitations 

* `zeph` is in very early development, and is not yet ready for use. 
* Only `zarr` stores with [consolidated metadata](https://zarr.readthedocs.io/en/latest/user-guide/consolidated_metadata/) are currently supported. 
