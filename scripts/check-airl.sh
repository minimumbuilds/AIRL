#!/bin/bash
# ~/check-airl.sh
ps aux | grep "airl-driver" | grep -v grep | awk '{print "CPU:", $3"%", "MEM:", $6/1024"MB", "TIME:", $10}' 2>/dev/null
