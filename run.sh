#!/usr/bin/env bash
current=$(pwd)
cd ./target/release/

for case in "case1" "case2_without_control" "case2_with_control" "case3_busy" "case3_not_busy"
do 
    echo Simulating case $case

    time ./simple ./Santiago.epw $case
done

echo Going back to $current
cd $current