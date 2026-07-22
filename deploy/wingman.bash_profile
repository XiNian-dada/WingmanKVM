# Show the same Armbian MOTD used by SSH logins.
if [ -r /run/motd.dynamic ]; then
    cat /run/motd.dynamic
elif [ -r /etc/motd ]; then
    cat /etc/motd
fi
