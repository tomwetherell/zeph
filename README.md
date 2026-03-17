# zeph

An interactive CLI tool to quickly and easily inspect zarr stores. Supports local and remote (S3, GCS, Azure, HTTPS) stores. 

![zeph_demo](https://github.com/user-attachments/assets/0bb46852-4191-47a9-b58d-8b4991e614d0)

## Getting Started 

### Install prebuilt binaries via shell script

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tomwetherell/zeph/releases/download/v0.2.0/zeph-installer.sh | sh
```

### Quickstart 

After installation, run `zeph <path to local or remote zarr store>` to start an interactive session. 

See the table below for example datasets which can be used for testing. 

| Name  | Backend | Path | 
| --- | --- | --- |
| [WeatherBench 2 - ERA 5](https://weatherbench2.readthedocs.io/en/latest/data-guide.html) | GCS | `gs://weatherbench2/datasets/era5/1959-2022-1h-240x121_equiangular_with_poles_conservative.zarr` | 
| [Multi-Scale Ultra High Resolution (MUR) Sea Surface Temperature (SST)](https://registry.opendata.aws/mur/) | S3 | `s3://mur-sst/zarr/` |

## Commands

| Command | Description |
| --- | --- |
| [`/summary`](https://github.com/tomwetherell/zeph?tab=readme-ov-file#summary) | Show an overview of the store (dimensions, variables and attributes) |
| [`/info`](https://github.com/tomwetherell/zeph?tab=readme-ov-file#info) | Show variable details (chunking information, coordinates, attributes, and more) |

### summary

<img width="1177" height="626" alt="image" src="https://github.com/user-attachments/assets/a5213049-fb00-4cbd-a87e-c0917db8995c" />

### info 

<img width="1177" height="610" alt="image" src="https://github.com/user-attachments/assets/fde349b3-f594-411a-a602-8088c1de3438" />

## Limitations 

* zarr stores without [consolidated metadata](https://zarr.readthedocs.io/en/latest/user-guide/consolidated_metadata/) are not supported. 
