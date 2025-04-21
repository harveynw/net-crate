#!/bin/bash

NUM_RUNS=5

for ((i=1; i<=NUM_RUNS; i++))
do
    python $(dirname "$0")/ws_rtc.py &
done