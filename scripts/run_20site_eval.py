#!/usr/bin/env python3
"""
CRFR 20-Site Evaluation — reproduces the protocol from crfr-20site-evaluation.json
using the local AetherAgent binary with fetch+js-eval features.

Protocol:
  - Phase BASELINE: Q1 (cold start, no feedback)
  - Phase TRAIN: Q2-Q7 (with feedback after each)
  - Phase TEST: Q8-Q10 (no feedback, measure generalization)

Output: JSON matching the original format + markdown summary.
"""

import json
import subprocess
import math
import sys
import os

# ─── Configuration ──────────────────────────────────────────────────────────

SITES = [
    {
        "name": "BBC News",
        "url": "https://www.bbc.com/news",
        "goals": [
            "latest news headlines today",
            "breaking news stories right now",
            "top news articles today",
            "current world news updates",
            "major news events happening now",
            "important world headlines today",
            "todays top news stories",
            "what is happening in the world right now",
            "global news and current events",
            "recent major world developments",
        ],
        "relevance_keywords": ["news", "article", "headline", "story", "report", "breaking"],
    },
    {
        "name": "NPR",
        "url": "https://www.npr.org/",
        "goals": [
            "latest news stories today",
            "breaking news headlines now",
            "top articles published today",
            "important current events",
            "major stories happening right now",
            "key news developments today",
            "most notable news stories",
            "what are todays biggest news stories",
            "current affairs and global events",
            "recent notable world happenings",
        ],
        "relevance_keywords": ["news", "article", "story", "report", "headline"],
    },
    {
        "name": "The Guardian",
        "url": "https://www.theguardian.com/us",
        "goals": [
            "latest news stories today",
            "breaking news and headlines",
            "top stories right now",
            "current world news coverage",
            "important events today",
            "major headlines today",
            "todays most read stories",
            "global news developments today",
            "current affairs and analysis",
            "recent world events and reports",
        ],
        "relevance_keywords": ["news", "story", "headline", "report", "article", "analysis"],
    },
    {
        "name": "GitHub Trending",
        "url": "https://github.com/trending",
        "goals": [
            "popular Rust repositories today",
            "trending Rust projects",
            "top Rust repos with most stars",
            "Rust language repositories trending now",
            "best Rust projects on GitHub",
            "most starred Rust repos today",
            "Rust repositories gaining traction",
            "open source Rust codebases trending",
            "Rust tools and libraries popular now",
            "which Rust projects are hot today",
        ],
        "relevance_keywords": ["rust", "repository", "repo", "star", "crate", "github"],
    },
    {
        "name": "Hacker News",
        "url": "https://news.ycombinator.com/",
        "goals": [
            "AI and machine learning news",
            "artificial intelligence stories today",
            "deep learning articles",
            "machine learning project news",
            "AI research and breakthroughs",
            "neural network developments",
            "latest AI tools and frameworks",
            "language model and LLM news",
            "computer vision advances today",
            "generative AI developments",
        ],
        "relevance_keywords": ["ai", "machine learning", "llm", "gpt", "neural", "model", "deep learning"],
    },
    {
        "name": "Stack Overflow",
        "url": "https://stackoverflow.com/questions",
        "goals": [
            "how to parse HTML in Rust",
            "Rust HTML parser library",
            "parsing HTML with Rust",
            "best Rust crate for HTML parsing",
            "html5ever Rust usage",
            "web scraping with Rust",
            "Rust DOM parsing library",
            "Rust crate for processing web pages",
            "extract data from HTML using Rust",
            "DOM manipulation Rust programming",
        ],
        "relevance_keywords": ["rust", "html", "parse", "dom", "scrape", "crate"],
    },
    {
        "name": "Wikipedia Einstein",
        "url": "https://en.wikipedia.org/wiki/Albert_Einstein",
        "goals": [
            "when was Einstein born",
            "Einstein birth date and place",
            "where was Albert Einstein born",
            "year Einstein was born",
            "Einstein early life birthplace",
            "born Albert Einstein date",
            "Einstein birth year and location",
            "what year was Einstein born and where",
            "Einstein origins and birthplace",
            "birth facts about Albert Einstein",
        ],
        "relevance_keywords": ["1879", "march", "ulm", "born", "germany", "birth"],
    },
    {
        "name": "Wikipedia Rust",
        "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        "goals": [
            "who created Rust programming language",
            "Rust language creator",
            "who invented Rust",
            "Rust programming origin story",
            "creator of the Rust language",
            "who designed Rust originally",
            "Rust language author and history",
            "who started the Rust project",
            "developer who made Rust",
            "Rust programming creation history",
        ],
        "relevance_keywords": ["graydon", "hoare", "2006", "2010", "mozilla", "created"],
    },
    {
        "name": "Wikipedia Python",
        "url": "https://en.wikipedia.org/wiki/Python_(programming_language)",
        "goals": [
            "who created Python programming language",
            "Python language creator",
            "who invented Python",
            "Python programming origin",
            "creator of Python language",
            "who designed Python",
            "Python language author",
            "who started the Python project",
            "developer who made Python",
            "Python creation history",
        ],
        "relevance_keywords": ["guido", "rossum", "1991", "cwi", "created", "designed"],
    },
    {
        "name": "Wikipedia Linux",
        "url": "https://en.wikipedia.org/wiki/Linux",
        "goals": [
            "who created Linux operating system",
            "Linux creator and history",
            "who invented Linux",
            "Linux kernel origin story",
            "creator of Linux OS",
            "who designed Linux",
            "Linux kernel author",
            "who started the Linux project",
            "developer who made Linux kernel",
            "Linux creation and history",
        ],
        "relevance_keywords": ["linus", "torvalds", "1991", "kernel", "created", "unix"],
    },
    {
        "name": "Amazon",
        "url": "https://www.amazon.com/",
        "goals": [
            "best selling laptops today",
            "popular laptop deals",
            "top rated laptops on sale",
            "laptop computer best deals",
            "affordable laptop recommendations",
            "most popular laptop brands",
            "best laptop value today",
            "laptop deals under 500 dollars",
            "budget friendly laptops",
            "student laptop recommendations",
        ],
        "relevance_keywords": ["laptop", "computer", "deal", "price", "$", "buy", "rating"],
    },
    {
        "name": "ESPN",
        "url": "https://www.espn.com/",
        "goals": [
            "latest sports scores today",
            "todays game results",
            "live sports scores and updates",
            "major sports results today",
            "current game scores",
            "todays match results",
            "sports scores and highlights",
            "what are todays sports results",
            "live game updates and scores",
            "current sports standings and scores",
        ],
        "relevance_keywords": ["score", "game", "win", "loss", "team", "match", "nba", "nfl", "mlb"],
    },
    {
        "name": "Weather.com",
        "url": "https://weather.com/",
        "goals": [
            "weather forecast today",
            "current weather conditions",
            "todays temperature forecast",
            "weather report for today",
            "local weather update",
            "hourly weather forecast",
            "weather conditions right now",
            "is it going to rain today",
            "temperature and weather today",
            "todays weather outlook and forecast",
        ],
        "relevance_keywords": ["temperature", "forecast", "rain", "weather", "degree", "wind", "humidity"],
    },
    {
        "name": "Yahoo Finance",
        "url": "https://finance.yahoo.com/",
        "goals": [
            "stock market today",
            "major stock indices performance",
            "market summary today",
            "S&P 500 performance today",
            "stock market news and updates",
            "Dow Jones today",
            "market trends and analysis",
            "how are stocks performing today",
            "current market movements",
            "stock exchange results today",
        ],
        "relevance_keywords": ["stock", "market", "s&p", "dow", "nasdaq", "index", "price", "%"],
    },
    {
        "name": "Allrecipes",
        "url": "https://www.allrecipes.com/",
        "goals": [
            "best pasta recipes",
            "popular pasta dinner recipes",
            "easy pasta recipes for tonight",
            "homemade pasta dish ideas",
            "quick pasta recipes for dinner",
            "classic pasta recipes",
            "best rated pasta recipes",
            "what pasta should I cook tonight",
            "simple and delicious pasta meals",
            "family pasta dinner ideas",
        ],
        "relevance_keywords": ["pasta", "recipe", "cook", "ingredient", "dinner", "minute", "rating"],
    },
    {
        "name": "Khan Academy",
        "url": "https://www.khanacademy.org/",
        "goals": [
            "learn mathematics online",
            "math courses and tutorials",
            "algebra lessons free",
            "mathematics learning resources",
            "online math education courses",
            "free math tutoring",
            "math practice and lessons",
            "where to learn math online free",
            "interactive math courses",
            "best math education platform",
        ],
        "relevance_keywords": ["math", "algebra", "course", "lesson", "learn", "practice", "tutorial"],
    },
    {
        "name": "USA.gov",
        "url": "https://www.usa.gov/",
        "goals": [
            "government benefits and services",
            "how to apply for government benefits",
            "federal services for citizens",
            "government assistance programs",
            "public benefits information",
            "citizen services overview",
            "federal government help",
            "what government services are available",
            "how to get government assistance",
            "public services and benefits guide",
        ],
        "relevance_keywords": ["benefit", "service", "government", "federal", "apply", "assistance"],
    },
    {
        "name": "Nature",
        "url": "https://www.nature.com/",
        "goals": [
            "latest scientific research",
            "new science publications today",
            "recent scientific discoveries",
            "important research papers",
            "breakthrough science news",
            "major scientific findings",
            "new research in science journals",
            "what are the latest scientific discoveries",
            "cutting edge research papers",
            "notable science publications this week",
        ],
        "relevance_keywords": ["research", "study", "science", "paper", "publish", "discover", "journal"],
    },
    {
        "name": "TripAdvisor",
        "url": "https://www.tripadvisor.com/",
        "goals": [
            "best hotels in New York",
            "top rated New York hotels",
            "popular hotels in NYC",
            "New York City hotel recommendations",
            "best places to stay in New York",
            "NYC hotel deals",
            "highly rated New York accommodations",
            "where to stay in New York City",
            "New York hotel reviews and ratings",
            "recommended hotels in Manhattan",
        ],
        "relevance_keywords": ["hotel", "new york", "nyc", "stay", "review", "rating", "room"],
    },
    {
        "name": "WebMD",
        "url": "https://www.webmd.com/",
        "goals": [
            "common cold symptoms and treatment",
            "cold flu symptoms guide",
            "how to treat a cold",
            "symptoms of common cold",
            "cold remedies and treatment",
            "what helps with a cold",
            "cold virus symptoms list",
            "home remedies for cold symptoms",
            "when to see doctor for cold",
            "cold versus flu symptoms difference",
        ],
        "relevance_keywords": ["cold", "symptom", "treatment", "fever", "cough", "flu", "remedy"],
    },
]

# ─── Relevance judgment ─────────────────────────────────────────────────────

def is_relevant(label, keywords):
    """Binary relevance: does the node label contain content keywords?"""
    lower = label.lower()
    # Filter out obvious nav/boilerplate
    nav_signals = ["cookie", "privacy", "sign in", "log in", "subscribe", "newsletter",
                   "skip to", "menu", "footer", "copyright", "terms of use"]
    for nav in nav_signals:
        if nav in lower and len(lower) < 100:
            return False
    return any(kw.lower() in lower for kw in keywords)


def ndcg_at_k(relevances, k=5):
    """Compute nDCG@k from binary relevance list."""
    relevances = relevances[:k]
    dcg = sum(rel / math.log2(i + 2) for i, rel in enumerate(relevances))
    ideal = sorted(relevances, reverse=True)
    idcg = sum(rel / math.log2(i + 2) for i, rel in enumerate(ideal))
    return dcg / idcg if idcg > 0 else 0.0


def mrr(relevances):
    """Mean Reciprocal Rank."""
    for i, rel in enumerate(relevances):
        if rel > 0:
            return 1.0 / (i + 1)
    return 0.0


def precision_at_k(relevances, k=5):
    return sum(relevances[:k]) / k


# ─── Run evaluation ─────────────────────────────────────────────────────────

def run_parse_crfr(url, goal, run_js=True):
    """Call aether parse_crfr via the library's Python-accessible path.
    Since we can't call Rust directly from Python, we'll use the CRFR benchmark
    approach: build a quick helper binary or use the HTTP server."""
    # We'll use curl against the deployed MCP server
    # Actually — we can't do that from this script directly.
    # Instead, output a structured test plan that can be executed via MCP tools.
    pass


def main():
    """Generate the evaluation plan as JSON for execution via MCP tools."""
    print(json.dumps({
        "protocol": "CRFR 20-Site Evaluation v2 (post-optimization)",
        "date": "2026-04-06",
        "sites": len(SITES),
        "queries_per_site": 10,
        "total_queries": len(SITES) * 10,
        "site_list": [{"name": s["name"], "url": s["url"]} for s in SITES],
    }, indent=2))


if __name__ == "__main__":
    main()
