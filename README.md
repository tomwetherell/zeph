# zeph

An interactive CLI tool to quickly and easily inspect zarr stores. Supports local and remote (S3, GCS, Azure, HTTPS) stores. 

https://github.com/user-attachments/assets/833d810b-629f-48ad-b656-d5f7ccaf2dea

## Getting Started 

### Install

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tomwetherell/zeph/releases/download/v0.3.1/zeph-installer.sh | sh
```

### Quickstart 

Run `zeph <path to local or remote zarr store>` to start an interactive session. 

See the table below for example datasets which can be used for testing. 

| Name  | Backend | Zarr Version | Path | 
| --- | --- | --- | --- |
| [WeatherBench 2 - ERA 5](https://weatherbench2.readthedocs.io/en/latest/data-guide.html) | GCS | 2 | `gs://weatherbench2/datasets/era5/1959-2022-1h-240x121_equiangular_with_poles_conservative.zarr` | 
| [Multi-Scale Ultra High Resolution (MUR) Sea Surface Temperature (SST)](https://registry.opendata.aws/mur/) | S3 | 2 | `s3://mur-sst/zarr/` |
| [NOAA GEFS from dynamical.org](https://dynamical.org/catalog/noaa-gefs-analysis/) | HTTPS | 3 | `https://data.dynamical.org/noaa/gefs/analysis/latest.zarr/` |

## Commands

| Command | Description |
| --- | --- |
| [`/summary`](https://github.com/tomwetherell/zeph?tab=readme-ov-file#summary) | Show an overview of the store (dimensions, variables and attributes) |
| [`/info`](https://github.com/tomwetherell/zeph?tab=readme-ov-file#info) | Show variable details (chunking, coordinates, attributes, etc.) |

### summary

<img width="1134" height="641" alt="image" src="https://github.com/user-attachments/assets/a0b7f767-940e-4715-a2c3-f49b33f06143" />

### info 

<img width="1134" height="641" alt="image" src="https://github.com/user-attachments/assets/901d521a-deb3-4897-9fa1-152866f5136b" />

<img width="1090" height="739" alt="image" src="https://github.com/user-attachments/assets/227bfa91-1581-4312-af47-64ab59c52e47" />

## Limitations 

* zarr stores without [consolidated metadata](https://zarr.readthedocs.io/en/latest/user-guide/consolidated_metadata/) are not supported. 
