#!/usr/bin/env python3
"""Creates (or resets) a game account on the local vmangos server, the way vmangos's own
`account create` does: an SRP6 salt and verifier in `realmd.account`, then a GM level.

    python3 tools/make_account.py <user> <password> [--gm 3] [--server server]
    python3 tools/make_account.py --check <user> <password>   # does the password match?

Talks to the database container of the vmangos-deploy checkout in `server/`
(`docker compose exec database mariadb`), with vmangos-deploy's default root password.
"""
import argparse, hashlib, os, secrets, subprocess, sys

N = int('894B645E89E1535BBDAD5B8B290650530801B18EBFBF5E8FAB3C82872A3E9BB7', 16)
G = 7


def verifier(user, password, salt):
    """vmangos `SRP6::CalculateVerifier`: x = SHA1(salt_le || SHA1(USER:PASS)) read
    little-endian, v = g^x mod N. Salt and verifier are stored as big-endian hex."""
    inner = hashlib.sha1(f'{user.upper()}:{password.upper()}'.encode()).digest()
    outer = hashlib.sha1(salt.to_bytes(32, 'little') + inner).digest()
    return pow(G, int.from_bytes(outer, 'little'), N)


def sql(server, query):
    out = subprocess.run(
        ['docker', 'compose', 'exec', '-T', 'database', 'mariadb', '-uroot', '-ppassword',
         '-N', '-B', 'realmd', '-e', query],
        cwd=server, check=True, capture_output=True, text=True)
    return out.stdout.strip()


def valid_name(name):
    return 0 < len(name) <= 16 and name.isascii() and name.isalnum()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('user')
    ap.add_argument('password')
    ap.add_argument('--gm', type=int, default=3)
    ap.add_argument('--server', default=os.path.join(os.path.dirname(__file__), '..', 'server'))
    ap.add_argument('--check', action='store_true')
    a = ap.parse_args()
    user, password = a.user.upper(), a.password
    if not valid_name(user) or not (0 < len(password) <= 16):
        sys.exit('user: letters and digits, up to 16; password: up to 16 characters')

    if a.check:
        row = sql(a.server, f"SELECT s, v FROM account WHERE username='{user}'")
        if not row:
            sys.exit(f'no account {user}')
        s_hex, v_hex = row.split('\t')
        ok = verifier(user, password, int(s_hex, 16)) == int(v_hex, 16)
        print('password matches' if ok else 'password does NOT match')
        sys.exit(0 if ok else 1)

    salt = secrets.randbits(256) | (1 << 255)
    s_hex, v_hex = f'{salt:064X}', f'{verifier(user, password, salt):X}'
    sql(a.server,
        f"INSERT INTO account (username, s, v, joindate) VALUES ('{user}', '{s_hex}', '{v_hex}', NOW()) "
        f"ON DUPLICATE KEY UPDATE s=VALUES(s), v=VALUES(v);"
        f"UPDATE account SET gmlevel={int(a.gm)} WHERE username='{user}';"
        "REPLACE INTO realmcharacters (realmid, acctid, numchars) SELECT realmlist.id, account.id, 0 "
        "FROM realmlist, account LEFT JOIN realmcharacters ON acctid=account.id WHERE acctid IS NULL;")
    print(f'account {user} ready (GM level {a.gm})')


if __name__ == '__main__':
    main()
