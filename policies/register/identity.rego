package identity

headers = {
    "Content-Type": "application/json",
    "Accept": "application/json"
}

url = sprintf("%s?medium=%s&address=%s", [data.external_service.url, "email", input.email])


get_identity_info = http.send(
    {
        "method": "get",
        "url": url,
        "headers": headers
    }
)