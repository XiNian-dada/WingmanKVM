# WingmanKVM interactive console login summary.
printf '\n\033[1;36mWingmanKVM\033[0m  %s\n' "$(uname -sr)"
printf '主机: %s   用户: %s\n' "$(hostname)" "$(id -un)"
printf '负载: %s\n' "$(cut -d' ' -f1-3 /proc/loadavg)"
printf '内存: '; free -h 2>/dev/null | awk '/^Mem:/ {print $3 " / " $2}'
printf '磁盘: '; df -h / 2>/dev/null | awk 'NR==2 {print $3 " / " $2 " (" $5 ")"}'
printf '\n'
