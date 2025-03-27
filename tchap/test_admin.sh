
CLIENT_ID=01J44RKQYM4G3TNVANTMTDYTX6
CLIENT_SECRET=phoo8ahneir3ohY2eigh4xuu6Oodaewi
#MAS_HOST=localhost:8080
MAS_HOST=auth.tchapgouv.com

ACCESS_TOKEN=$(curl \
  -u "$CLIENT_ID:$CLIENT_SECRET" \
  -d "grant_type=client_credentials&scope=urn:mas:admin" \
  https://$MAS_HOST/oauth2/token \
  | jq -r '.access_token')

echo $ACCESS_TOKEN

# List users (The -g flag prevents curl from interpreting the brackets in the URL)
curl \
   -g \
   -H "Authorization: Bearer $ACCESS_TOKEN" \
   "https://$MAS_HOST/api/admin/v1/users?filter[can_request_admin]=true&filter[status]=active&page[first]=100" \
   | jq
