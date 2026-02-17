FROM python:3.14-trixie AS build
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt -t /usr/local/lib/python3.14/site-packages

FROM python:3.14-slim-trixie
WORKDIR /app
COPY --from=build /usr/local/lib/python3.14/site-packages /usr/local/lib/python3.14/site-packages
COPY server .
CMD ["python", "server.py"]
