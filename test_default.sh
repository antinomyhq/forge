#!/bin/bash                                                       

# Configure timeout for curl requests


# Start the server (if not already running)                       
# cargo run -p forge_server &                                     
# SERVER_PID=$!                                                   
# sleep 2  # Give the server time to start                        

echo "===== Testing Server Connection ====="
# First, test if the server is running by making a simple request
if ! curl -s --max-time 5 http://127.0.0.1:3000/api/health 2>/dev/null; then
    echo "Warning: Server health check failed. Make sure the server is running at http://127.0.0.1:3000"
    echo "You can start the server with: cargo run -p forge_server"
    echo "Continuing anyway..."
fi

# Create a conversation with default workflow                                          
echo ""
echo "===== Creating Conversation with Default Workflow ====="                                   
CONVERSATION_RESPONSE=$(curl -s -X POST \
http://127.0.0.1:3000/api/init \
  -H "Content-Type: application/json")

# The server returns just a UUID in quotes, strip the quotes to get the ID
CONVERSATION_ID=$(echo $CONVERSATION_RESPONSE | tr -d '"')
                                                                  
echo "Conversation created with ID: $CONVERSATION_ID"             

# Verify the conversation ID is not empty
if [ -z "$CONVERSATION_ID" ]; then
  echo "Error: Failed to extract conversation ID from response"
  exit 1
fi

echo ""
echo "===== Trying Chat Request with Verbose Output ====="
# Try with verbose output first to see the full HTTP transaction
echo "Attempting chat request with verbose output..."
curl -v -N -X POST \
http://127.0.0.1:3000/api/chat \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d "{
    \"event\": {
      \"id\": \"$(uuidgen)\",
      \"name\": \"user_task_init\",
      \"value\": \"Hello, how are you today?\",
      \"timestamp\": \"$(date -u +"%Y-%m-%dT%H:%M:%SZ")\"
    },
    \"conversation_id\": \"$CONVERSATION_ID\"
  }"
# Cleanup (uncomment if you started the server in this script)    
# kill $SERVER_PID