####################################################################################
###
### kaspa-miner
### https://github.com/tmrlvi/kaspa-miner/releases
###
### Hive integration: Merlin
###
####################################################################################

#!/usr/bin/env bash

#######################
# MAIN script body
#######################

. /hive/miners/custom/kaspa-miner/h-manifest.conf
 stats_raw=`cat $CUSTOM_LOG_BASENAME.log | grep -w "hashrate" | tail -n 1 `

#Calculate miner log freshness

maxDelay=120
time_now=`date +%T | awk -F: '{ print ($1 * 3600) + $2*60 + $3 }'`
time_rep=`echo $stats_raw | awk -FT '{print $2}' | awk -FZ '{print $1}' | awk -F: '{ print (($1+1)*3600) + $2*60 + $3}'`
diffTime=`echo $((time_now-time_rep)) | tr -d '-'`

if [ "$diffTime" -lt "$maxDelay" ]; then
        total_hashrate=`echo $stats_raw | awk '{print $7}' | cut -d "." -f 1,2 --output-delimiter='' | sed 's/$/0/'`
	if [[ $stats_raw == *"Ghash"* ]]; then
		total_hashrate=$(($total_hashrate*1000))
	fi
        stats=$(jq -nc \
                --argjson hs "[$total_hashrate]"\
                --arg ver "$CUSTOM_VERSION" \
                --arg ths "$total_hashrate" \
                '{ hs: $hs, hs_units: "khs", algo : "heavyhash", ver:$ver }')
        khs=$total_hashrate
else
  khs=0
  stats="null"
fi

[[ -z $khs ]] && khs=0
[[ -z $stats ]] && stats="null"
