"""Convert GMT's gridline-registered NetCDF DEM to the bundled elevation grid.

Requires netCDF4 and numpy. Usage: prepare-terrain.py input.grd output.bin
"""

import argparse
import struct
from pathlib import Path

import netCDF4
import numpy as np


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    with netCDF4.Dataset(args.source) as data:
        lon, lat = data["lon"][:], data["lat"][:]
        step = float(lon[1] - lon[0])
        if not (np.allclose(np.diff(lon), step) and np.allclose(np.diff(lat), step)):
            raise ValueError("Expected a regular longitude/latitude grid")
        elevation = data["z"][:]
        if np.ma.getmaskarray(elevation).any():
            raise ValueError("Elevation grid contains missing samples")

        # Store south-to-north rows as signed whole meters; no NetCDF runtime needed.
        header = struct.pack("<8sIIddd", b"STRDEM01", len(lon), len(lat), lon[0], lat[0], step)
        args.output.write_bytes(header + np.rint(elevation).astype("<i2").tobytes())


if __name__ == "__main__":
    main()
