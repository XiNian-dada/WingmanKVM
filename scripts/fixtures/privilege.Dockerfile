FROM ubuntu:24.04@sha256:24737cef17f4e22b1977df7d9a05e77c4e28b7da5b8911f8d8ccda136bf3573c

# Disposable sudo-policy verifier only; never used for the product deployment.
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
       bash coreutils passwd python3 sudo util-linux \
    && printf 'WingmanKVM disposable policy fixture\n' > /wingmankvm-disposable-test

WORKDIR /repo
CMD ["python3", "scripts/verify_p2c_privileges.py", "--repo", "/repo", "--disposable-root"]
