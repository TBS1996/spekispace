#!/bin/bash

(
  cd ./speki-proxy || exit 1
  echo "Starting speki-proxy..."
  cargo run
) &

(
  cd ./speki-auth || exit 1
  echo "Starting speki-auth..."
  cargo run
) &

(
  cd ./speki-web || exit 1
  echo "Starting tailwindcss..."
  npx tailwindcss -i ./input.css -o ./public/tailwind.css --watch
) &

wait

echo "All services stopped."

