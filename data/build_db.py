"""One-shot script to build hyrr.sqlite from raw data sources.

Parses:
- TENDL cross-sections from isotopia.libs/{p,d,t,h,a}/*/iaea.2024/tables/residual/*
- PSTAR/ASTAR stopping power tables from libdEdx data files
- Natural isotopic abundances (IUPAC)
- Decay data from ISOTOPIA decay files

Usage:
    python data/build_db.py --tendl-path ../curie/isotopia.libs/ --output data/hyrr.sqlite
"""
