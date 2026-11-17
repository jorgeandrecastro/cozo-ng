#!/usr/bin/env bash
# Smoke test du serveur cozo-bin (HTTP API).
#
# Usage :
#   # serveur local sans auth (bind 127.0.0.1) :
#   ./cozo-bin server &                       # puis :
#   tests/smoke.sh
#
#   # serveur avec auth (Docker, bind 0.0.0.0) :
#   COZO_AUTH=<token> tests/smoke.sh http://127.0.0.1:9070
#
#   # test du timeout des transactions (lancer le serveur avec --tx-timeout 5) :
#   SLOW=1 tests/smoke.sh
set -u

BASE="${1:-http://127.0.0.1:9070}"
AUTH="${COZO_AUTH:-}"
REL="smoke_$$"
PASS=0
FAIL=0
BODY="$(mktemp)"
trap 'rm -f "$BODY"' EXIT

# req METHOD PATH [JSON] [extra curl args...]  -> met le code HTTP dans $CODE et le corps dans $BODY
req() {
  local method="$1" path="$2" data="${3:-}"
  shift 3 2>/dev/null || shift $#
  local args=(-s -o "$BODY" -w '%{http_code}' -X "$method" "$BASE$path" -H 'content-type: application/json')
  [ -n "$AUTH" ] && args+=(-H "x-cozo-auth: $AUTH")
  [ -n "$data" ] && args+=(-d "$data")
  CODE="$(curl "${args[@]}" "$@")"
}

check() { # check "nom" attendu_code [motif_dans_le_corps]
  local name="$1" want="$2" pat="${3:-}"
  if [ "$CODE" = "$want" ] && { [ -z "$pat" ] || grep -q "$pat" "$BODY"; }; then
    echo "ok   - $name"; PASS=$((PASS + 1))
  else
    echo "FAIL - $name (code $CODE, attendu $want) : $(head -c 200 "$BODY")"; FAIL=$((FAIL + 1))
  fi
}

# --- Authentification -------------------------------------------------------
req POST /text-query '{"script":"?[] <- [[1]]","params":{}}'
check "requête simple" 200 '"ok":true'

if [ -n "$AUTH" ]; then
  CODE="$(curl -s -o "$BODY" -w '%{http_code}' -X POST "$BASE/text-query" \
    -H 'content-type: application/json' -d '{"script":"?[] <- [[1]]","params":{}}')"
  check "sans token -> 401" 401

  CODE="$(curl -s -o "$BODY" -w '%{http_code}' -X POST "$BASE/text-query" \
    -H 'content-type: application/json' -H 'x-cozo-auth: mauvais' \
    -d '{"script":"?[] <- [[1]]","params":{}}')"
  check "mauvais token -> 401" 401

  CODE="$(curl -s -o "$BODY" -w '%{http_code}' -X POST "$BASE/text-query?auth=$AUTH" \
    -H 'content-type: application/json' -d '{"script":"?[] <- [[1]]","params":{}}')"
  check "token via ?auth=" 200 '"ok":true'
fi

# --- Relation : create / put / read / immutable -----------------------------
req POST /text-query "{\"script\":\":create $REL {k: Int => v: String}\",\"params\":{}}"
check "create relation" 200 '"ok":true'

req POST /text-query "{\"script\":\"?[k, v] <- [[1, 'a']] :put $REL {k => v}\",\"params\":{}}"
check "put" 200 '"ok":true'

req POST /text-query "{\"script\":\"?[k, v] := *$REL{k, v}\",\"params\":{}}"
check "lecture" 200 '"a"'

req POST /text-query "{\"script\":\"?[k, v] <- [[2, 'b']] :put $REL {k => v}\",\"params\":{},\"immutable\":true}"
check "écriture refusée en immutable" 400

# --- Import / export --------------------------------------------------------
req GET "/export/$REL"
check "export" 200 '"ok":true'

# --- Transaction multi-requêtes : commit ------------------------------------
req POST "/transact?write=true"
check "start transaction" 200 '"id"'
TX="$(sed -n 's/.*"id":\([0-9]*\).*/\1/p' "$BODY")"

req POST "/transact/$TX" "{\"script\":\"?[k, v] <- [[3, 'c']] :put $REL {k => v}\",\"params\":{}}"
check "requête dans la transaction" 200 '"rows"'

req PUT "/transact/$TX" '{"abort":false}'
check "commit" 200 '"ok":true'

req POST "/transact/$TX" '{"script":"?[] <- [[1]]","params":{}}'
check "transaction commitée -> 404" 404

req POST "/text-query" "{\"script\":\"?[k] := *$REL{k, v}, k = 3\",\"params\":{}}"
check "donnée commitée visible" 200 '\[3\]'

# --- Transaction : abort ----------------------------------------------------
req POST "/transact?write=true"
TX="$(sed -n 's/.*"id":\([0-9]*\).*/\1/p' "$BODY")"
req POST "/transact/$TX" "{\"script\":\"?[k, v] <- [[4, 'd']] :put $REL {k => v}\",\"params\":{}}"
req PUT "/transact/$TX" '{"abort":true}'
check "abort" 200 '"ok":true'

req POST "/text-query" "{\"script\":\"?[k] := *$REL{k, v}, k = 4\",\"params\":{}}"
if grep -q '\[4\]' "$BODY"; then
  echo "FAIL - donnée avortée visible"; FAIL=$((FAIL + 1))
else
  echo "ok   - donnée avortée absente"; PASS=$((PASS + 1))
fi

# --- Timeout des transactions (serveur lancé avec --tx-timeout 5) ------------
if [ "${SLOW:-0}" = "1" ]; then
  req POST "/transact?write=true"
  TX="$(sed -n 's/.*"id":\([0-9]*\).*/\1/p' "$BODY")"
  echo "attente de 45 s pour l'expiration de la transaction $TX ..."
  sleep 45
  req POST "/transact/$TX" '{"script":"?[] <- [[1]]","params":{}}'
  check "transaction expirée -> 404" 404
fi

# --- Nettoyage --------------------------------------------------------------
req POST /text-query "{\"script\":\"::remove $REL\",\"params\":{}}"
check "nettoyage" 200 '"ok":true'

echo
echo "Résultat : $PASS réussis, $FAIL échoués"
[ "$FAIL" -eq 0 ]